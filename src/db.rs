use crate::models::{
    Bookmark, BookmarkPayload, BookmarkSaveResponse, CountValue, DuplicateBookmarkResponse,
};
use url::Url;
use worker::d1::{D1Database, D1Type};
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
