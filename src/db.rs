use crate::models::{
    Bookmark, BookmarkPayload, BookmarkSaveResponse, BulkBookmarksPayload, BulkBookmarksResponse,
    CountValue, DeleteBookmarksResponse, DeleteFolderResponse, DuplicateBookmarkResponse,
    FolderOrder, FolderPayload, IdsPayload, MoveBookmarksPayload, MoveBookmarksResponse,
    MoveFolderUpResponse, RenameFolderPayload, RenameFolderResponse, ReorderBookmarksPayload,
    ReorderBookmarksResponse, ReorderFoldersPayload, ReorderFoldersResponse, WebdavConfig,
    WebdavConfigPayload, WebdavConfigResponse,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;
use worker::d1::{D1Database, D1Result, D1Type};
use worker::*;

pub const D1_BINDING: &str = "DB";
const DEFAULT_WEBDAV_FILENAME: &str = "linkwise-bookmarks.html";
const SCHEMA_VERSION_KEY: &str = "schema.version";
const SCHEMA_VERSION: &str = "2026-06-19-initial";
static SCHEMA_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub async fn initialize_schema(db: &D1Database) -> Result<()> {
    if SCHEMA_INITIALIZED.load(Ordering::Relaxed) {
        return Ok(());
    }

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS bookmarks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            folder TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmarks_url
        ON bookmarks (url)
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE INDEX IF NOT EXISTS idx_bookmarks_folder_order
        ON bookmarks (folder, sort_order)
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        )
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS folder_orders (
            parent_folder TEXT NOT NULL DEFAULT '',
            folder_name TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (parent_folder, folder_name)
        )
        "#,
    )
    .run()
    .await?;

    let version_args = [
        D1Type::Text(SCHEMA_VERSION_KEY),
        D1Type::Text(SCHEMA_VERSION),
    ];
    db.prepare(
        r#"
        INSERT OR REPLACE INTO settings (key, value)
        VALUES (?, ?)
        "#,
    )
    .bind_refs(&version_args)?
    .run()
    .await?;

    SCHEMA_INITIALIZED.store(true, Ordering::Relaxed);

    Ok(())
}

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

pub async fn reorder_folders(
    db: &D1Database,
    payload: ReorderFoldersPayload,
) -> Result<ReorderFoldersResponse> {
    let parent_folder = normalize_folder_path(&payload.parent_folder.unwrap_or_default());
    let folder_names = payload
        .folders
        .into_iter()
        .filter_map(|item| {
            let parts = split_folder_path(&item);
            parts.last().cloned().or_else(|| {
                let trimmed = item.trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            })
        })
        .collect::<Vec<_>>();
    let existing_names = folder_names_under(db, &parent_folder).await?;
    let existing_name_set = existing_names.iter().cloned().collect::<HashSet<_>>();
    let mut valid_names = Vec::new();

    for name in folder_names {
        if existing_name_set.contains(&name) && !valid_names.contains(&name) {
            valid_names.push(name);
        }
    }

    let mut ordered_names = valid_names;
    for name in existing_names {
        if !ordered_names.contains(&name) {
            ordered_names.push(name);
        }
    }

    for (sort_order, name) in ordered_names.iter().enumerate() {
        let args = [
            D1Type::Text(&parent_folder),
            D1Type::Text(name),
            D1Type::Integer(sort_order as i32),
        ];
        db.prepare(
            r#"
            INSERT OR REPLACE INTO folder_orders (parent_folder, folder_name, sort_order)
            VALUES (?, ?, ?)
            "#,
        )
        .bind_refs(&args)?
        .run()
        .await?;
    }

    Ok(ReorderFoldersResponse {
        status: "success",
        parent_folder,
        updated_count: ordered_names.len(),
    })
}

pub async fn move_folder_up(
    db: &D1Database,
    payload: FolderPayload,
) -> Result<Result<MoveFolderUpResponse, (u16, serde_json::Value)>> {
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());

    if folder.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("请选择要操作的目录"))?,
        )));
    }

    let parts = split_folder_path(&folder);
    let parent_folder = parts[..parts.len().saturating_sub(1)].join(" / ");
    let rows = bookmark_folder_rows_in_branch(db, &folder).await?;
    let mut moved_count = 0;

    for row in rows {
        let Some(new_folder) = move_folder_path_up(&row.folder, &folder, &parent_folder) else {
            continue;
        };
        let args = [D1Type::Text(&new_folder), D1Type::Text(&row.id)];
        db.prepare("UPDATE bookmarks SET folder = ? WHERE id = ?")
            .bind_refs(&args)?
            .run()
            .await?;
        moved_count += 1;
    }

    sync_folder_order_move_up(db, &folder, &parent_folder).await?;

    Ok(Ok(MoveFolderUpResponse {
        status: "success",
        message: "目录已删除，书签已移动到上一层",
        moved_count,
        parent_folder,
    }))
}

pub async fn rename_folder(
    db: &D1Database,
    payload: RenameFolderPayload,
) -> Result<Result<RenameFolderResponse, (u16, serde_json::Value)>> {
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());
    let new_folder = normalize_folder_path(&payload.new_folder.unwrap_or_default());

    if folder.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("请选择要操作的目录"))?,
        )));
    }

    if folder == new_folder {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("新目录和原目录相同"))?,
        )));
    }

    let rows = bookmark_folder_rows_in_branch(db, &folder).await?;
    let mut renamed_count = 0;

    for row in rows {
        let updated_folder = replace_folder_path(&row.folder, &folder, &new_folder);

        if updated_folder == row.folder {
            continue;
        }

        let args = [D1Type::Text(&updated_folder), D1Type::Text(&row.id)];
        db.prepare("UPDATE bookmarks SET folder = ? WHERE id = ?")
            .bind_refs(&args)?
            .run()
            .await?;
        renamed_count += 1;
    }

    sync_folder_order_rename(db, &folder, &new_folder).await?;

    Ok(Ok(RenameFolderResponse {
        status: "success",
        message: "目录已更新",
        renamed_count,
        folder,
        new_folder,
    }))
}

pub async fn delete_folder(
    db: &D1Database,
    payload: FolderPayload,
) -> Result<Result<DeleteFolderResponse, (u16, serde_json::Value)>> {
    let folder = normalize_folder_path(&payload.folder.unwrap_or_default());

    if folder.is_empty() {
        return Ok(Err((
            400,
            serde_json::to_value(crate::models::ErrorResponse::new("请选择要操作的目录"))?,
        )));
    }

    let like = folder_like_pattern(&folder);
    let args = [D1Type::Text(&folder), D1Type::Text(&like)];
    let result = db
        .prepare("DELETE FROM bookmarks WHERE folder = ? OR folder LIKE ?")
        .bind_refs(&args)?
        .run()
        .await?;

    delete_folder_order_branch(db, &folder).await?;

    Ok(Ok(DeleteFolderResponse {
        status: "success",
        message: "目录和目录下书签已删除",
        deleted_count: result_changes(&result)?,
    }))
}

pub async fn webdav_config(db: &D1Database) -> Result<WebdavConfigResponse> {
    Ok(WebdavConfigResponse {
        status: "success",
        config: build_webdav_config(get_settings(db, WEBDAV_SETTING_KEYS).await?),
    })
}

pub async fn update_webdav_config(
    db: &D1Database,
    payload: WebdavConfigPayload,
    secret: Option<String>,
) -> Result<Result<WebdavConfigResponse, (u16, serde_json::Value)>> {
    let filename = payload
        .filename
        .unwrap_or_else(|| DEFAULT_WEBDAV_FILENAME.to_string())
        .trim()
        .to_string();
    let filename = if filename.is_empty() {
        DEFAULT_WEBDAV_FILENAME.to_string()
    } else {
        filename
    };
    let password = payload.password.unwrap_or_default();
    let mut values = vec![
        (
            "webdav_url".to_string(),
            payload.webdav_url.unwrap_or_default().trim().to_string(),
        ),
        (
            "webdav_username".to_string(),
            payload.username.unwrap_or_default().trim().to_string(),
        ),
        (
            "webdav_remote_dir".to_string(),
            payload.remote_dir.unwrap_or_default().trim().to_string(),
        ),
        ("webdav_filename".to_string(), filename),
    ];

    if !password.is_empty() {
        let Some(secret) = secret.filter(|value| !value.trim().is_empty()) else {
            return Ok(Err((
                500,
                serde_json::to_value(crate::models::ErrorResponse::new(
                    "未配置 LINKWISE_SECRET，无法保存 WebDAV 密码",
                ))?,
            )));
        };

        values.push((
            "webdav_password_ciphertext".to_string(),
            crate::crypto::protect_webdav_password(&password, &secret),
        ));
        values.push((
            "webdav_password_nonce".to_string(),
            "sha256-worker-secret-v1".to_string(),
        ));
    }

    save_settings(db, &values).await?;

    if !password.is_empty() {
        delete_settings(db, &["webdav_password"]).await?;
    }

    webdav_config(db).await.map(Ok)
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

async fn folder_names_under(db: &D1Database, parent_folder: &str) -> Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct FolderNameValue {
        folder_name: String,
    }

    let args = [D1Type::Text(parent_folder)];
    let rows = db
        .prepare(
            r#"
            SELECT folder_name
            FROM folder_orders
            WHERE parent_folder = ?
            ORDER BY sort_order ASC, folder_name ASC
            "#,
        )
        .bind_refs(&args)?
        .all()
        .await?
        .results::<FolderNameValue>()?;
    Ok(rows.into_iter().map(|row| row.folder_name).collect())
}

#[derive(serde::Deserialize)]
struct BookmarkFolderRow {
    id: String,
    folder: String,
}

async fn bookmark_folder_rows_in_branch(
    db: &D1Database,
    folder: &str,
) -> Result<Vec<BookmarkFolderRow>> {
    let like = folder_like_pattern(folder);
    let args = [D1Type::Text(folder), D1Type::Text(&like)];
    db.prepare("SELECT id, folder FROM bookmarks WHERE folder = ? OR folder LIKE ?")
        .bind_refs(&args)?
        .all()
        .await?
        .results()
}

async fn all_folder_order_rows(db: &D1Database) -> Result<Vec<FolderOrder>> {
    db.prepare("SELECT parent_folder, folder_name, sort_order FROM folder_orders")
        .all()
        .await?
        .results()
}

async fn sync_folder_order_rename(db: &D1Database, folder: &str, new_folder: &str) -> Result<()> {
    let rows = all_folder_order_rows(db).await?;

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);
        let updated_path = replace_folder_path(&current_path, folder, new_folder);

        if updated_path == current_path {
            continue;
        }

        delete_folder_order_row(db, &row.parent_folder, &row.folder_name).await?;
        let updated_parts = split_folder_path(&updated_path);

        if updated_parts.is_empty() {
            continue;
        }

        let updated_parent = updated_parts[..updated_parts.len() - 1].join(" / ");
        let updated_name = updated_parts.last().cloned().unwrap_or_default();
        insert_folder_order_row(db, &updated_parent, &updated_name, row.sort_order).await?;
    }

    ensure_folder_order(db, new_folder).await
}

async fn delete_folder_order_branch(db: &D1Database, folder: &str) -> Result<()> {
    let rows = all_folder_order_rows(db).await?;

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);

        if current_path == folder || current_path.starts_with(&folder_child_prefix(folder)) {
            delete_folder_order_row(db, &row.parent_folder, &row.folder_name).await?;
        }
    }

    Ok(())
}

async fn sync_folder_order_move_up(
    db: &D1Database,
    folder: &str,
    parent_folder: &str,
) -> Result<()> {
    let rows = all_folder_order_rows(db).await?;

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);

        if current_path == folder {
            delete_folder_order_row(db, &row.parent_folder, &row.folder_name).await?;
            continue;
        }

        if !current_path.starts_with(&folder_child_prefix(folder)) {
            continue;
        }

        let suffix = &current_path[folder_child_prefix(folder).len()..];
        let updated_path = join_folder_path(parent_folder, suffix);
        let updated_parts = split_folder_path(&updated_path);
        delete_folder_order_row(db, &row.parent_folder, &row.folder_name).await?;

        if updated_parts.is_empty() {
            continue;
        }

        let updated_parent = updated_parts[..updated_parts.len() - 1].join(" / ");
        let updated_name = updated_parts.last().cloned().unwrap_or_default();
        insert_folder_order_row(db, &updated_parent, &updated_name, row.sort_order).await?;
    }

    Ok(())
}

async fn delete_folder_order_row(
    db: &D1Database,
    parent_folder: &str,
    folder_name: &str,
) -> Result<()> {
    let args = [D1Type::Text(parent_folder), D1Type::Text(folder_name)];
    db.prepare("DELETE FROM folder_orders WHERE parent_folder = ? AND folder_name = ?")
        .bind_refs(&args)?
        .run()
        .await?;
    Ok(())
}

async fn insert_folder_order_row(
    db: &D1Database,
    parent_folder: &str,
    folder_name: &str,
    sort_order: i64,
) -> Result<()> {
    let args = [
        D1Type::Text(parent_folder),
        D1Type::Text(folder_name),
        D1Type::Integer(sort_order as i32),
    ];
    db.prepare(
        r#"
        INSERT OR REPLACE INTO folder_orders (parent_folder, folder_name, sort_order)
        VALUES (?, ?, ?)
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;
    Ok(())
}

fn replace_folder_path(folder_path: &str, old_prefix: &str, new_prefix: &str) -> String {
    if folder_path == old_prefix {
        return new_prefix.to_string();
    }

    let prefix = folder_child_prefix(old_prefix);
    if folder_path.starts_with(&prefix) {
        let suffix = &folder_path[prefix.len()..];
        return join_folder_path(new_prefix, suffix);
    }

    folder_path.to_string()
}

fn move_folder_path_up(folder_path: &str, folder: &str, parent_folder: &str) -> Option<String> {
    if folder_path == folder {
        return Some(parent_folder.to_string());
    }

    let prefix = folder_child_prefix(folder);
    if folder_path.starts_with(&prefix) {
        let suffix = &folder_path[prefix.len()..];
        return Some(join_folder_path(parent_folder, suffix));
    }

    None
}

fn join_folder_path(parent: &str, name: &str) -> String {
    match (parent.is_empty(), name.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => parent.to_string(),
        (false, false) => format!("{parent} / {name}"),
    }
}

fn folder_child_prefix(folder: &str) -> String {
    format!("{folder} / ")
}

fn folder_like_pattern(folder: &str) -> String {
    format!("{}%", folder_child_prefix(folder))
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

const WEBDAV_SETTING_KEYS: &[&str] = &[
    "webdav_url",
    "webdav_username",
    "webdav_password",
    "webdav_password_ciphertext",
    "webdav_password_nonce",
    "webdav_remote_dir",
    "webdav_filename",
];

#[derive(serde::Deserialize)]
struct SettingRow {
    key: String,
    value: String,
}

async fn get_settings(db: &D1Database, keys: &[&str]) -> Result<HashMap<String, String>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }

    let query = format!(
        "SELECT key, value FROM settings WHERE key IN ({})",
        placeholders(keys.len())
    );
    let args = keys.iter().map(|key| D1Type::Text(key)).collect::<Vec<_>>();
    let rows = db
        .prepare(query)
        .bind_refs(&args)?
        .all()
        .await?
        .results::<SettingRow>()?;

    Ok(rows.into_iter().map(|row| (row.key, row.value)).collect())
}

async fn save_settings(db: &D1Database, values: &[(String, String)]) -> Result<()> {
    for (key, value) in values {
        let args = [D1Type::Text(key), D1Type::Text(value)];
        db.prepare(
            r#"
            INSERT OR REPLACE INTO settings (key, value)
            VALUES (?, ?)
            "#,
        )
        .bind_refs(&args)?
        .run()
        .await?;
    }

    Ok(())
}

async fn delete_settings(db: &D1Database, keys: &[&str]) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let query = format!(
        "DELETE FROM settings WHERE key IN ({})",
        placeholders(keys.len())
    );
    let args = keys.iter().map(|key| D1Type::Text(key)).collect::<Vec<_>>();
    db.prepare(query).bind_refs(&args)?.run().await?;
    Ok(())
}

fn build_webdav_config(settings: HashMap<String, String>) -> WebdavConfig {
    let has_encrypted_password = settings
        .get("webdav_password_ciphertext")
        .filter(|value| !value.is_empty())
        .is_some()
        && settings
            .get("webdav_password_nonce")
            .filter(|value| !value.is_empty())
            .is_some();
    let has_legacy_password = settings
        .get("webdav_password")
        .filter(|value| !value.is_empty())
        .is_some();

    WebdavConfig {
        webdav_url: settings.get("webdav_url").cloned().unwrap_or_default(),
        username: settings.get("webdav_username").cloned().unwrap_or_default(),
        remote_dir: settings
            .get("webdav_remote_dir")
            .cloned()
            .unwrap_or_default(),
        filename: settings
            .get("webdav_filename")
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| DEFAULT_WEBDAV_FILENAME.to_string()),
        has_password: has_encrypted_password || has_legacy_password,
        password_security: if has_encrypted_password {
            "worker_secret_hash"
        } else if has_legacy_password {
            "legacy_plaintext"
        } else {
            "empty"
        },
    }
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
