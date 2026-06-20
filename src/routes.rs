use crate::models::{ErrorResponse, HealthResponse};
use worker::*;

pub async fn handle(req: Request, env: Env) -> Result<Response> {
    Router::new()
        .get_async("/api/health", |_req, _ctx| async move {
            Response::from_json(&HealthResponse::success())
        })
        .get_async("/api/bookmarks", |_req, _ctx| async move {
            not_implemented("GET /api/bookmarks is pending the D1 migration")
        })
        .get_async("/api/folder-orders", |_req, _ctx| async move {
            not_implemented("GET /api/folder-orders is pending the D1 migration")
        })
        .get_async("/api/bookmarks/export", |_req, _ctx| async move {
            not_implemented("GET /api/bookmarks/export is pending the D1 migration")
        })
        .get_async("/api/webdav/config", |_req, _ctx| async move {
            not_implemented("GET /api/webdav/config is pending the Worker secret migration")
        })
        .run(req, env)
        .await
}

fn not_implemented(message: &str) -> Result<Response> {
    let response = Response::from_json(&ErrorResponse::new(message))?.with_status(501);
    Ok(response)
}
