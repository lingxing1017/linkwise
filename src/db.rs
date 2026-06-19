use crate::models::{
    Bookmark, BookmarkPayload, BookmarkSaveResponse, BulkBookmarksPayload, BulkBookmarksResponse,
    CountValue, DeleteBookmarksResponse, DuplicateBookmarkResponse, FolderOrder, IdsPayload,
    MoveBookmarksPayload, MoveBookmarksResponse, ReorderBookmarksPayload, ReorderBookmarksResponse,
};
use std::collections::{HashMap, HashSet};
use url::Url;
use worker::d1::{D1Database, D1Result, D1Type};
use worker::*;

pub const D1_BINDING: &str = "DB";

pub async fn all_bookmarks(db: &D1Database) -> Result<Vec<Bookmark>> {
    db.prepare(
        r#"
        SELECT id, title, url, folder, sort_order
        FROM bookmarks
        ORDER BY folder ASC, sort_order ASC, rowid DESC
        "#,
    )
    .all()
    .await?
    .results()
}

pub async fn all_folder_orders(db: &D1Database) -> Result<Vec<FolderOrder>> {
    db.prepare(
        r#"
        SELECT parent_folder, folder_name, sort_order
        FROM folder_orders
        ORDER BY parent_folder ASC, sort_order ASC, folder_name ASC
        "#,
    )
    .all()
    .await?
    .results()
}

pub async fn save_bookmark(
    db: &D1Database,
    payload: BookmarkPayload,
) -> Result<Result<BookmarkSaveResponse, (u16, serde_json::Value)>> {
    let id = payload
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(timestamp_id);
    let title = payload.title.unwrap_or_default().trim().to_string();
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());
    let url = normalize_url(payload.url.as_deref().unwrap_or_default());

    if title.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::with_code(
                "missing_title",
                "标题不能为空",
            ))?,
        )));
    }

    let Some(url) = url else {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::with_code(
                "invalid_url",
                "URL 无效",
            ))?,
        )));
    };

    let duplicate_args = [D1Type::Text(&url), D1Type::Text(&id)];
    let duplicate = db
        .prepare(
            r#"
            SELECT id, title, url, folder, sort_order
            FROM bookmarks
            WHERE url = ? AND id != ?
            LIMIT 1
            "#,
        )
        .bind_refs(&duplicate_args)?
        .first::<Bookmark>(None)
        .await?;

    if let Some(bookmark) = duplicate {
        return Ok(Err((
            409,
            serde_json::to_value(DuplicateBookmarkResponse {
                status: "error",
                error: "duplicate_url",
                message: "这个 URL 已存在",
                bookmark,
            })?,
        )));
    }

    let existing_args = [D1Type::Text(&id)];
    let existing = db
        .prepare("SELECT folder, sort_order AS value FROM bookmarks WHERE id = ?")
        .bind_refs(&existing_args)?
        .first::<ExistingBookmarkOrder>(None)
        .await?;
    let sort_order = match existing {
        Some(existing) if existing.folder == folder => existing.value,
        _ => next_bookmark_sort_order(db, &folder).await?,
    };

    ensure_folder_order(db, &folder).await?;

    let save_args = [
        D1Type::Text(&id),
        D1Type::Text(&title),
        D1Type::Text(&url),
        D1Type::Text(&folder),
        D1Type::Integer(sort_order as i32),
    ];
    db.prepare(
        r#"
        INSERT OR REPLACE INTO bookmarks (id, title, url, folder, sort_order)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind_refs(&save_args)?
    .run()
    .await?;

    let total_count = count_bookmarks(db).await?;

    Ok(Ok(BookmarkSaveResponse {
        status: "success",
        id,
        title,
        url,
        folder,
        total_count,
    }))
}

pub async fn bulk_save_bookmarks(
    db: &D1Database,
    payload: BulkBookmarksPayload,
) -> Result<Result<BulkBookmarksResponse, (u16, serde_json::Value)>> {
    let mut valid_items = Vec::new();
    let mut imported_ids = Vec::new();
    let mut skipped_count = 0;
    let mut duplicate_count = 0;
    let mut seen_urls = HashSet::new();
    let now = timestamp_id();

    let existing_urls = existing_urls(db).await?;
    let mut folder_next_orders = HashMap::<String, i64>::new();

    for (index, item) in payload.bookmarks.into_iter().enumerate() {
        let id = item
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{now}-{index}"));
        let title = item
            .title
            .unwrap_or_else(|| "未命名书签".to_string())
            .trim()
            .to_string();
        let folder = normalize_folder_path(&item.folder.unwrap_or_default());
        let url = normalize_url(item.url.as_deref().unwrap_or_default());

        let Some(url) = url else {
            skipped_count += 1;
            continue;
        };

        if title.is_empty() {
            skipped_count += 1;
            continue;
        }

        if existing_urls.contains(&url) || seen_urls.contains(&url) {
            duplicate_count += 1;
            continue;
        }

        seen_urls.insert(url.clone());

        if !folder_next_orders.contains_key(&folder) {
            folder_next_orders.insert(folder.clone(), next_bookmark_sort_order(db, &folder).await?);
        }

        let sort_order = folder_next_orders.get(&folder).copied().unwrap_or(0);
        folder_next_orders.insert(folder.clone(), sort_order + 1);
        ensure_folder_order(db, &folder).await?;

        imported_ids.push(id.clone());
        valid_items.push((id, title, url, folder, sort_order));
    }

    for (id, title, url, folder, sort_order) in &valid_items {
        let args = [
            D1Type::Text(id),
            D1Type::Text(title),
            D1Type::Text(url),
            D1Type::Text(folder),
            D1Type::Integer(*sort_order as i32),
        ];
        db.prepare(
            r#"
            INSERT OR REPLACE INTO bookmarks (id, title, url, folder, sort_order)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind_refs(&args)?
        .run()
        .await?;
    }

    Ok(Ok(BulkBookmarksResponse {
        status: "success",
        imported_count: valid_items.len(),
        imported_ids,
        duplicate_count,
        skipped_count,
        total_count: count_bookmarks(db).await?,
    }))
}

pub async fn move_bookmarks(
    db: &D1Database,
    payload: MoveBookmarksPayload,
) -> Result<Result<MoveBookmarksResponse, (u16, serde_json::Value)>> {
    let ids = clean_ids(payload.ids);
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());

    if ids.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("请选择要移动的书签"))?,
        )));
    }

    ensure_folder_order(db, &folder).await?;
    let next_order = next_bookmark_sort_order(db, &folder).await?;
    let mut moved_count = 0;

    for (index, id) in ids.iter().enumerate() {
        let sort_order = next_order + index as i64;
        let args = [
            D1Type::Text(&folder),
            D1Type::Integer(sort_order as i32),
            D1Type::Text(id),
        ];
        let result = db
            .prepare("UPDATE bookmarks SET folder = ?, sort_order = ? WHERE id = ?")
            .bind_refs(&args)?
            .run()
            .await?;
        moved_count += result_changes(&result)?;
    }

    Ok(Ok(MoveBookmarksResponse {
        status: "success",
        moved_count,
        folder,
    }))
}

pub async fn reorder_bookmarks(
    db: &D1Database,
    payload: ReorderBookmarksPayload,
) -> Result<ReorderBookmarksResponse> {
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());
    let requested_ids = clean_ids(payload.ids);
    let existing_ids = bookmark_ids_in_folder(db, &folder).await?;
    let existing_id_set = existing_ids.iter().cloned().collect::<HashSet<_>>();
    let mut valid_ids = Vec::new();

    for id in requested_ids {
        if existing_id_set.contains(&id) && !valid_ids.contains(&id) {
            valid_ids.push(id);
        }
    }

    let mut ordered_ids = valid_ids;
    for id in existing_ids {
        if !ordered_ids.contains(&id) {
            ordered_ids.push(id);
        }
    }

    for (sort_order, id) in ordered_ids.iter().enumerate() {
        let args = [D1Type::Integer(sort_order as i32), D1Type::Text(id)];
        db.prepare("UPDATE bookmarks SET sort_order = ? WHERE id = ?")
            .bind_refs(&args)?
            .run()
            .await?;
    }

    Ok(ReorderBookmarksResponse {
        status: "success",
        folder,
        updated_count: ordered_ids.len(),
    })
}

pub async fn delete_bookmarks(
    db: &D1Database,
    payload: IdsPayload,
) -> Result<Result<DeleteBookmarksResponse, (u16, serde_json::Value)>> {
    let ids = clean_ids(payload.ids);

    if ids.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("请选择要删除的书签"))?,
        )));
    }

    let placeholders = placeholders(ids.len());
    let query = format!("DELETE FROM bookmarks WHERE id IN ({placeholders})");
    let args = ids.iter().map(|id| D1Type::Text(id)).collect::<Vec<_>>();
    let result = db.prepare(query).bind_refs(&args)?.run().await?;

    Ok(Ok(DeleteBookmarksResponse {
        status: "success",
        deleted_count: result_changes(&result)?,
    }))
}

pub async fn delete_bookmark(db: &D1Database, id: &str) -> Result<serde_json::Value> {
    let args = [D1Type::Text(id)];
    db.prepare("DELETE FROM bookmarks WHERE id = ?")
        .bind_refs(&args)?
        .run()
        .await?;
    Ok(serde_json::json!({ "status": "success" }))
}

#[derive(serde::Deserialize)]
struct ExistingBookmarkOrder {
    folder: String,
    value: i64,
}

async fn next_bookmark_sort_order(db: &D1Database, folder: &str) -> Result<i64> {
    let args = [D1Type::Text(folder)];
    let row = db
        .prepare(
            r#"
            SELECT COALESCE(MAX(sort_order), -1) + 1 AS value
            FROM bookmarks
            WHERE folder = ?
            "#,
        )
        .bind_refs(&args)?
        .first::<CountValue>(None)
        .await?;
    Ok(row.map(|row| row.value).unwrap_or(0))
}

async fn count_bookmarks(db: &D1Database) -> Result<i64> {
    let row = db
        .prepare("SELECT COUNT(*) AS value FROM bookmarks")
        .first::<CountValue>(None)
        .await?;
    Ok(row.map(|row| row.value).unwrap_or(0))
}

#[derive(serde::Deserialize)]
struct UrlValue {
    url: String,
}

async fn existing_urls(db: &D1Database) -> Result<HashSet<String>> {
    let rows = db
        .prepare("SELECT url FROM bookmarks WHERE url IS NOT NULL")
        .all()
        .await?
        .results::<UrlValue>()?;
    Ok(rows
        .into_iter()
        .filter_map(|row| (!row.url.is_empty()).then_some(row.url))
        .collect())
}

#[derive(serde::Deserialize)]
struct IdValue {
    id: String,
}

async fn bookmark_ids_in_folder(db: &D1Database, folder: &str) -> Result<Vec<String>> {
    let args = [D1Type::Text(folder)];
    let rows = db
        .prepare(
            r#"
            SELECT id
            FROM bookmarks
            WHERE folder = ?
            ORDER BY sort_order ASC, rowid DESC
            "#,
        )
        .bind_refs(&args)?
        .all()
        .await?
        .results::<IdValue>()?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}

fn clean_ids(ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

fn placeholders(count: usize) -> String {
    std::iter::repeat("?")
        .take(count)
        .collect::<Vec<_>>()
        .join(",")
}

fn result_changes(result: &D1Result) -> Result<usize> {
    Ok(result
        .meta()?
        .and_then(|meta| meta.changes)
        .unwrap_or_default())
}

async fn ensure_folder_order(db: &D1Database, folder: &str) -> Result<()> {
    let parts = split_folder_path(folder);

    for index in 0..parts.len() {
        let name = &parts[index];
        let parent_folder = parts[..index].join(" / ");
        let exists_args = [D1Type::Text(&parent_folder), D1Type::Text(name)];
        let exists = db
            .prepare(
                r#"
                SELECT 1 AS value
                FROM folder_orders
                WHERE parent_folder = ? AND folder_name = ?
                LIMIT 1
                "#,
            )
            .bind_refs(&exists_args)?
            .first::<CountValue>(None)
            .await?;

        if exists.is_some() {
            continue;
        }

        let next_args = [D1Type::Text(&parent_folder)];
        let next_order = db
            .prepare(
                r#"
                SELECT COALESCE(MAX(sort_order), -1) + 1 AS value
                FROM folder_orders
                WHERE parent_folder = ?
                "#,
            )
            .bind_refs(&next_args)?
            .first::<CountValue>(None)
            .await?
            .map(|row| row.value)
            .unwrap_or(0);

        let insert_args = [
            D1Type::Text(&parent_folder),
            D1Type::Text(name),
            D1Type::Integer(next_order as i32),
        ];
        db.prepare(
            r#"
            INSERT INTO folder_orders (parent_folder, folder_name, sort_order)
            VALUES (?, ?, ?)
            "#,
        )
        .bind_refs(&insert_args)?
        .run()
        .await?;
    }

    Ok(())
}

pub fn normalize_folder_path(folder: &str) -> String {
    split_folder_path(folder).join(" / ")
}

pub fn split_folder_path(folder: &str) -> Vec<String> {
    folder
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_url(url: &str) -> Option<String> {
    let url = url.trim();

    if url.is_empty() {
        return None;
    }

    let lowered = url.to_lowercase();
    let candidate = if lowered.starts_with("http://") || lowered.starts_with("https://") {
        url.to_string()
    } else if url.contains("://") {
        return None;
    } else {
        format!("https://{url}")
    };

    is_valid_url(&candidate).then_some(candidate)
}

fn is_valid_url(url: &str) -> bool {
    match Url::parse(url) {
        Ok(parsed) => matches!(parsed.scheme(), "http" | "https") && parsed.host().is_some(),
        Err(_) => false,
    }
}

fn timestamp_id() -> String {
    (js_sys::Date::now() as i64).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_folder_path() {
        assert_eq!(normalize_folder_path(" Dev / Rust "), "Dev / Rust");
        assert_eq!(normalize_folder_path("///"), "");
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_url("example.com:8080/path"),
            Some("https://example.com:8080/path".to_string())
        );
        assert_eq!(normalize_url("javascript:alert(1)"), None);
        assert_eq!(normalize_url("chrome-extension://abcdef"), None);
    }
}
