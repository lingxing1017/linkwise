use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "linkwise";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub app: &'static str,
    pub version: &'static str,
}

impl HealthResponse {
    pub fn success() -> Self {
        Self {
            status: "success",
            app: APP_NAME,
            version: APP_VERSION,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    pub url: String,
    pub folder: String,
    pub sort_order: i64,
}

#[derive(Serialize)]
pub struct BootstrapResponse {
    pub bookmarks: Vec<Bookmark>,
    pub folder_orders: Vec<FolderOrder>,
}

#[derive(Debug, Default, Deserialize)]
pub struct BookmarkPayload {
    pub id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct BulkBookmarksPayload {
    #[serde(default)]
    pub bookmarks: Vec<BookmarkPayload>,
}

#[derive(Deserialize)]
pub struct CountValue {
    pub value: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminCredential {
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: i64,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthChallenge {
    pub id: String,
    pub challenge: String,
    pub purpose: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSession {
    pub id: String,
    pub token_hash: String,
    pub credential_id: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Serialize)]
pub struct BookmarkSaveResponse {
    pub status: &'static str,
    pub id: String,
    pub title: String,
    pub url: String,
    pub folder: String,
    pub total_count: i64,
}

#[derive(Serialize)]
pub struct BulkBookmarksResponse {
    pub status: &'static str,
    pub imported_count: usize,
    pub imported_ids: Vec<String>,
    pub duplicate_count: usize,
    pub skipped_count: usize,
    pub total_count: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct IdsPayload {
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MoveBookmarksPayload {
    #[serde(default)]
    pub ids: Vec<String>,
    pub folder: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReorderBookmarksPayload {
    pub folder: Option<String>,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReorderFoldersPayload {
    pub parent_folder: Option<String>,
    #[serde(default)]
    pub folders: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FolderPayload {
    pub folder: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RenameFolderPayload {
    pub folder: Option<String>,
    pub new_folder: Option<String>,
}

#[derive(Serialize)]
pub struct MoveBookmarksResponse {
    pub status: &'static str,
    pub moved_count: usize,
    pub folder: String,
}

#[derive(Serialize)]
pub struct ReorderBookmarksResponse {
    pub status: &'static str,
    pub folder: String,
    pub updated_count: usize,
}

#[derive(Serialize)]
pub struct DeleteBookmarksResponse {
    pub status: &'static str,
    pub deleted_count: usize,
}

#[derive(Serialize)]
pub struct ReorderFoldersResponse {
    pub status: &'static str,
    pub parent_folder: String,
    pub updated_count: usize,
}

#[derive(Serialize)]
pub struct MoveFolderUpResponse {
    pub status: &'static str,
    pub message: &'static str,
    pub moved_count: usize,
    pub parent_folder: String,
}

#[derive(Serialize)]
pub struct RenameFolderResponse {
    pub status: &'static str,
    pub message: &'static str,
    pub renamed_count: usize,
    pub folder: String,
    pub new_folder: String,
}

#[derive(Serialize)]
pub struct DeleteFolderResponse {
    pub status: &'static str,
    pub message: &'static str,
    pub deleted_count: usize,
}

#[derive(Debug, Default, Deserialize)]
pub struct WebdavConfigPayload {
    pub webdav_url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub remote_dir: Option<String>,
    pub filename: Option<String>,
}

#[derive(Serialize)]
pub struct WebdavConfig {
    pub webdav_url: String,
    pub username: String,
    pub remote_dir: String,
    pub filename: String,
    pub has_password: bool,
    pub password_security: &'static str,
}

#[derive(Serialize)]
pub struct WebdavConfigResponse {
    pub status: &'static str,
    pub config: WebdavConfig,
}

#[derive(Serialize)]
pub struct DuplicateBookmarkResponse {
    pub status: &'static str,
    pub error: &'static str,
    pub message: &'static str,
    pub bookmark: Bookmark,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FolderOrder {
    pub parent_folder: String,
    pub folder_name: String,
    pub sort_order: i64,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'static str>,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            error: None,
            message: message.into(),
        }
    }

    pub fn with_code(error: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: "error",
            error: Some(error),
            message: message.into(),
        }
    }
}
