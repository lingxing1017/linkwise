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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FolderOrder {
    pub parent_folder: String,
    pub folder_name: String,
    pub sort_order: i64,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            message: message.into(),
        }
    }
}
