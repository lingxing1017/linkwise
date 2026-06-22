use crate::models::{
    Bookmark, BookmarkPayload, BookmarkSaveResponse, BootstrapResponse, BulkBookmarksPayload,
    BulkBookmarksResponse, CountValue, DeleteBookmarksResponse, DeleteFolderResponse,
    DuplicateBookmarkResponse, FolderOrder, FolderPayload, IdsPayload, MoveBookmarksPayload,
    MoveBookmarksResponse, MoveFolderUpResponse, RenameFolderPayload, RenameFolderResponse,
    ReorderBookmarksPayload, ReorderBookmarksResponse, ReorderFoldersPayload,
    ReorderFoldersResponse, WebdavConfig, WebdavConfigPayload, WebdavConfigResponse,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;
use worker::d1::{
    D1Database, D1PreparedArgument as WorkerD1PreparedArgument, D1PreparedStatement, D1Result,
    D1Type,
};
use worker::*;

pub const D1_BINDING: &str = "DB";
const DEFAULT_WEBDAV_FILENAME: &str = "linkwise-bookmarks.html";
const SCHEMA_VERSION_KEY: &str = "schema.version";
const SCHEMA_VERSION: &str = "2026-06-19-initial";
const D1_MAX_BIND_PARAMS: usize = 100;
const ALL_BOOKMARKS_SQL: &str = r#"
    SELECT id, title, url, folder, sort_order
    FROM bookmarks
    ORDER BY folder ASC, sort_order ASC, rowid DESC
    "#;
const ALL_FOLDER_ORDERS_SQL: &str = r#"
    SELECT parent_folder, folder_name, sort_order
    FROM folder_orders
    ORDER BY parent_folder ASC, sort_order ASC, folder_name ASC
    "#;
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

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS admin_credentials (
            credential_id TEXT PRIMARY KEY,
            public_key TEXT NOT NULL,
            sign_count INTEGER NOT NULL DEFAULT 0,
            name TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            last_used_at INTEGER
        )
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS auth_challenges (
            id TEXT PRIMARY KEY,
            challenge TEXT NOT NULL,
            purpose TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            used_at INTEGER
        )
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE INDEX IF NOT EXISTS idx_auth_challenges_purpose_expires
        ON auth_challenges (purpose, expires_at)
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS admin_sessions (
            id TEXT PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            credential_id TEXT,
            created_at INTEGER NOT NULL,
            last_seen_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            revoked_at INTEGER
        )
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE INDEX IF NOT EXISTS idx_admin_sessions_credential_id
        ON admin_sessions (credential_id)
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires_revoked
        ON admin_sessions (expires_at, revoked_at)
        "#,
    )
    .run()
    .await?;

    db.prepare(
        r#"
        CREATE TABLE IF NOT EXISTS auth_rate_limits (
            bucket TEXT PRIMARY KEY,
            failed_count INTEGER NOT NULL DEFAULT 0,
            first_failed_at INTEGER NOT NULL,
            last_failed_at INTEGER NOT NULL,
            locked_until INTEGER
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
    db.prepare(ALL_BOOKMARKS_SQL)
    .all()
    .await?
    .results()
}

pub async fn all_folder_orders(db: &D1Database) -> Result<Vec<FolderOrder>> {
    db.prepare(ALL_FOLDER_ORDERS_SQL)
    .all()
    .await?
    .results()
}

pub async fn bootstrap_data(db: &D1Database) -> Result<BootstrapResponse> {
    let mut results = db
        .batch(vec![
            db.prepare(ALL_BOOKMARKS_SQL),
            db.prepare(ALL_FOLDER_ORDERS_SQL),
        ])
        .await?
        .into_iter();

    let bookmarks = match results.next() {
        Some(result) => result.results::<Bookmark>()?,
        None => Vec::new(),
    };
    let folder_orders = match results.next() {
        Some(result) => result.results::<FolderOrder>()?,
        None => Vec::new(),
    };

    Ok(BootstrapResponse {
        bookmarks,
        folder_orders,
    })
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
    let mut bookmark_next_orders = bookmark_next_orders(db).await?;
    let folder_orders = all_folder_orders(db).await?;
    let mut existing_folder_orders = HashSet::<(String, String)>::new();
    let mut folder_next_orders = HashMap::<String, i64>::new();
    let mut folder_order_items = Vec::<(String, String, i64)>::new();

    for row in folder_orders {
        existing_folder_orders.insert((row.parent_folder.clone(), row.folder_name.clone()));
        let next_order = folder_next_orders
            .entry(row.parent_folder)
            .or_insert(row.sort_order + 1);
        *next_order = (*next_order).max(row.sort_order + 1);
    }

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

        let sort_order = bookmark_next_orders.get(&folder).copied().unwrap_or(0);
        bookmark_next_orders.insert(folder.clone(), sort_order + 1);
        collect_missing_folder_orders(
            &folder,
            &mut existing_folder_orders,
            &mut folder_next_orders,
            &mut folder_order_items,
        );

        imported_ids.push(id.clone());
        valid_items.push((id, title, url, folder, sort_order));
    }

    insert_folder_orders_batch(db, &folder_order_items).await?;
    insert_bookmarks_batch(db, &valid_items).await?;

    Ok(Ok(BulkBookmarksResponse {
        status: "success",
        imported_count: valid_items.len(),
        imported_ids,
        duplicate_count,
        skipped_count,
        total_count: count_bookmarks(db).await?,
    }))
}

async fn insert_bookmarks_batch(
    db: &D1Database,
    valid_items: &[(String, String, String, String, i64)],
) -> Result<()> {
    const BINDS_PER_ROW: usize = 5;

    for chunk in valid_items.chunks(sql_chunk_size(BINDS_PER_ROW, 0)) {
        let placeholders = repeat_row_placeholders(chunk.len(), BINDS_PER_ROW);
        let query = format!(
            r#"
            INSERT OR REPLACE INTO bookmarks (id, title, url, folder, sort_order)
            VALUES {placeholders}
            "#
        );
        let mut values = Vec::with_capacity(chunk.len() * BINDS_PER_ROW);

        for (id, title, url, folder, sort_order) in chunk {
            values.push(D1Type::Text(id));
            values.push(D1Type::Text(title));
            values.push(D1Type::Text(url));
            values.push(D1Type::Text(folder));
            values.push(D1Type::Integer(*sort_order as i32));
        }
        let args = prepared_args(&values);

        db.prepare(query).bind_refs(&args)?.run().await?;
    }

    Ok(())
}

async fn insert_folder_orders_batch(
    db: &D1Database,
    folder_order_items: &[(String, String, i64)],
) -> Result<()> {
    const BINDS_PER_ROW: usize = 3;

    for chunk in folder_order_items.chunks(sql_chunk_size(BINDS_PER_ROW, 0)) {
        let placeholders = repeat_row_placeholders(chunk.len(), BINDS_PER_ROW);
        let query = format!(
            r#"
            INSERT OR IGNORE INTO folder_orders (parent_folder, folder_name, sort_order)
            VALUES {placeholders}
            "#
        );
        let mut values = Vec::with_capacity(chunk.len() * BINDS_PER_ROW);

        for (parent_folder, folder_name, sort_order) in chunk {
            values.push(D1Type::Text(parent_folder));
            values.push(D1Type::Text(folder_name));
            values.push(D1Type::Integer(*sort_order as i32));
        }
        let args = prepared_args(&values);

        db.prepare(query).bind_refs(&args)?.run().await?;
    }

    Ok(())
}

async fn update_bookmark_sort_orders_batch(db: &D1Database, ordered_ids: &[String]) -> Result<()> {
    let prepared = db.prepare("UPDATE bookmarks SET sort_order = ? WHERE id = ?");
    let mut statements = Vec::new();

    for (sort_order, id) in ordered_ids.iter().enumerate() {
        let args = [D1Type::Integer(sort_order as i32), D1Type::Text(id)];
        statements.push(prepared.bind_refs(&args)?);
    }

    run_statement_batches(db, statements, 2).await
}

async fn update_folder_sort_orders_batch(
    db: &D1Database,
    parent_folder: &str,
    ordered_names: &[String],
) -> Result<()> {
    let prepared = db.prepare(
        r#"
        INSERT OR REPLACE INTO folder_orders (parent_folder, folder_name, sort_order)
        VALUES (?, ?, ?)
        "#,
    );
    let mut statements = Vec::new();

    for (sort_order, name) in ordered_names.iter().enumerate() {
        let args = [
            D1Type::Text(parent_folder),
            D1Type::Text(name),
            D1Type::Integer(sort_order as i32),
        ];
        statements.push(prepared.bind_refs(&args)?);
    }

    run_statement_batches(db, statements, 3).await
}

async fn update_bookmark_folders_batch(
    db: &D1Database,
    updates: &[(String, String)],
) -> Result<usize> {
    let prepared = db.prepare("UPDATE bookmarks SET folder = ? WHERE id = ?");
    let mut statements = Vec::new();

    for (id, folder) in updates {
        let args = [D1Type::Text(folder), D1Type::Text(id)];
        statements.push(prepared.bind_refs(&args)?);
    }

    let mut updated_count = 0;
    for result in run_statement_batches_with_results(db, statements, 2).await? {
        updated_count += result_changes(&result)?;
    }

    Ok(updated_count)
}

async fn delete_folder_order_rows_batch(db: &D1Database, rows: &[(String, String)]) -> Result<()> {
    let prepared =
        db.prepare("DELETE FROM folder_orders WHERE parent_folder = ? AND folder_name = ?");
    let mut statements = Vec::new();

    for (parent_folder, folder_name) in rows {
        let args = [D1Type::Text(parent_folder), D1Type::Text(folder_name)];
        statements.push(prepared.bind_refs(&args)?);
    }

    run_statement_batches(db, statements, 2).await
}

async fn upsert_folder_order_rows_batch(
    db: &D1Database,
    rows: &[(String, String, i64)],
) -> Result<()> {
    let prepared = db.prepare(
        r#"
        INSERT OR REPLACE INTO folder_orders (parent_folder, folder_name, sort_order)
        VALUES (?, ?, ?)
        "#,
    );
    let mut statements = Vec::new();

    for (parent_folder, folder_name, sort_order) in rows {
        let args = [
            D1Type::Text(parent_folder),
            D1Type::Text(folder_name),
            D1Type::Integer(*sort_order as i32),
        ];
        statements.push(prepared.bind_refs(&args)?);
    }

    run_statement_batches(db, statements, 3).await
}

fn collect_missing_folder_orders(
    folder: &str,
    existing_folder_orders: &mut HashSet<(String, String)>,
    folder_next_orders: &mut HashMap<String, i64>,
    folder_order_items: &mut Vec<(String, String, i64)>,
) {
    let parts = split_folder_path(folder);

    for index in 0..parts.len() {
        let folder_name = parts[index].clone();
        let parent_folder = parts[..index].join(" / ");
        let key = (parent_folder.clone(), folder_name.clone());

        if existing_folder_orders.contains(&key) {
            continue;
        }

        let sort_order = folder_next_orders.get(&parent_folder).copied().unwrap_or(0);
        folder_next_orders.insert(parent_folder.clone(), sort_order + 1);
        existing_folder_orders.insert(key);
        folder_order_items.push((parent_folder, folder_name, sort_order));
    }
}

fn sql_chunk_size(binds_per_row: usize, fixed_bind_count: usize) -> usize {
    let available = D1_MAX_BIND_PARAMS.saturating_sub(fixed_bind_count);
    (available / binds_per_row).max(1)
}

fn prepared_args<'a>(values: &'a [D1Type<'a>]) -> Vec<WorkerD1PreparedArgument<'a>> {
    values.iter().map(WorkerD1PreparedArgument::new).collect()
}

async fn run_statement_batches(
    db: &D1Database,
    statements: Vec<D1PreparedStatement>,
    binds_per_statement: usize,
) -> Result<()> {
    for chunk in statements.chunks(sql_chunk_size(binds_per_statement, 0)) {
        db.batch(chunk.to_vec()).await?;
    }

    Ok(())
}

async fn run_statement_batches_with_results(
    db: &D1Database,
    statements: Vec<D1PreparedStatement>,
    binds_per_statement: usize,
) -> Result<Vec<D1Result>> {
    let mut results = Vec::new();

    for chunk in statements.chunks(sql_chunk_size(binds_per_statement, 0)) {
        results.extend(db.batch(chunk.to_vec()).await?);
    }

    Ok(results)
}

fn repeat_row_placeholders(row_count: usize, binds_per_row: usize) -> String {
    let row = format!("({})", vec!["?"; binds_per_row].join(", "));
    vec![row; row_count].join(", ")
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
    let prepared = db.prepare("UPDATE bookmarks SET folder = ?, sort_order = ? WHERE id = ?");
    let mut statements = Vec::new();

    for (index, id) in ids.iter().enumerate() {
        let sort_order = next_order + index as i64;
        let args = [
            D1Type::Text(&folder),
            D1Type::Integer(sort_order as i32),
            D1Type::Text(id),
        ];
        statements.push(prepared.bind_refs(&args)?);
    }

    for result in run_statement_batches_with_results(db, statements, 3).await? {
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

    update_bookmark_sort_orders_batch(db, &ordered_ids).await?;

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

    let mut deleted_count = 0;

    for chunk in ids.chunks(sql_chunk_size(1, 0)) {
        let placeholders = placeholders(chunk.len());
        let query = format!("DELETE FROM bookmarks WHERE id IN ({placeholders})");
        let args = chunk.iter().map(|id| D1Type::Text(id)).collect::<Vec<_>>();
        let result = db.prepare(query).bind_refs(&args)?.run().await?;
        deleted_count += result_changes(&result)?;
    }

    Ok(Ok(DeleteBookmarksResponse {
        status: "success",
        deleted_count,
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

    update_folder_sort_orders_batch(db, &parent_folder, &ordered_names).await?;

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
    let mut updates = Vec::new();

    for row in rows {
        let Some(new_folder) = move_folder_path_up(&row.folder, &folder, &parent_folder) else {
            continue;
        };
        updates.push((row.id, new_folder));
    }
    let moved_count = update_bookmark_folders_batch(db, &updates).await?;

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
    let mut updates = Vec::new();

    for row in rows {
        let updated_folder = replace_folder_path(&row.folder, &folder, &new_folder);

        if updated_folder == row.folder {
            continue;
        }

        updates.push((row.id, updated_folder));
    }
    let renamed_count = update_bookmark_folders_batch(db, &updates).await?;

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
struct FolderNextOrderValue {
    folder: String,
    value: i64,
}

async fn bookmark_next_orders(db: &D1Database) -> Result<HashMap<String, i64>> {
    let rows = db
        .prepare(
            r#"
            SELECT folder, COALESCE(MAX(sort_order), -1) + 1 AS value
            FROM bookmarks
            GROUP BY folder
            "#,
        )
        .all()
        .await?
        .results::<FolderNextOrderValue>()?;
    Ok(rows
        .into_iter()
        .map(|row| (row.folder, row.value))
        .collect())
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
    let mut rows_to_delete = Vec::new();
    let mut rows_to_upsert = Vec::new();

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);
        let updated_path = replace_folder_path(&current_path, folder, new_folder);

        if updated_path == current_path {
            continue;
        }

        rows_to_delete.push((row.parent_folder, row.folder_name));
        let updated_parts = split_folder_path(&updated_path);

        if updated_parts.is_empty() {
            continue;
        }

        let updated_parent = updated_parts[..updated_parts.len() - 1].join(" / ");
        let updated_name = updated_parts.last().cloned().unwrap_or_default();
        rows_to_upsert.push((updated_parent, updated_name, row.sort_order));
    }

    delete_folder_order_rows_batch(db, &rows_to_delete).await?;
    upsert_folder_order_rows_batch(db, &rows_to_upsert).await?;
    ensure_folder_order(db, new_folder).await
}

async fn delete_folder_order_branch(db: &D1Database, folder: &str) -> Result<()> {
    let rows = all_folder_order_rows(db).await?;
    let mut rows_to_delete = Vec::new();

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);

        if current_path == folder || current_path.starts_with(&folder_child_prefix(folder)) {
            rows_to_delete.push((row.parent_folder, row.folder_name));
        }
    }

    delete_folder_order_rows_batch(db, &rows_to_delete).await
}

async fn sync_folder_order_move_up(
    db: &D1Database,
    folder: &str,
    parent_folder: &str,
) -> Result<()> {
    let rows = all_folder_order_rows(db).await?;
    let mut rows_to_delete = Vec::new();
    let mut rows_to_upsert = Vec::new();

    for row in rows {
        let current_path = join_folder_path(&row.parent_folder, &row.folder_name);

        if current_path == folder {
            rows_to_delete.push((row.parent_folder, row.folder_name));
            continue;
        }

        if !current_path.starts_with(&folder_child_prefix(folder)) {
            continue;
        }

        let suffix = &current_path[folder_child_prefix(folder).len()..];
        let updated_path = join_folder_path(parent_folder, suffix);
        let updated_parts = split_folder_path(&updated_path);
        rows_to_delete.push((row.parent_folder, row.folder_name));

        if updated_parts.is_empty() {
            continue;
        }

        let updated_parent = updated_parts[..updated_parts.len() - 1].join(" / ");
        let updated_name = updated_parts.last().cloned().unwrap_or_default();
        rows_to_upsert.push((updated_parent, updated_name, row.sort_order));
    }

    delete_folder_order_rows_batch(db, &rows_to_delete).await?;
    upsert_folder_order_rows_batch(db, &rows_to_upsert).await
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
