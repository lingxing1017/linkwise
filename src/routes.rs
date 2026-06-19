use crate::db;
use crate::models::{BookmarkPayload, ErrorResponse, HealthResponse};
use worker::*;

pub async fn handle(req: Request, env: Env) -> Result<Response> {
    Router::new()
        .get_async("/api/health", |_req, _ctx| async move {
            Response::from_json(&HealthResponse::success())
        })
        .get_async("/api/bookmarks", |_req, ctx| async move {
            let db = ctx.env.d1(db::D1_BINDING)?;
            Response::from_json(&db::all_bookmarks(&db).await?)
        })
        .post_async("/api/bookmarks", |mut req, ctx| async move {
            let db = ctx.env.d1(db::D1_BINDING)?;
            let payload = req.json::<BookmarkPayload>().await.unwrap_or_default();

            match db::save_bookmark(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
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

fn json_with_status(value: &serde_json::Value, status: u16) -> Result<Response> {
    Ok(Response::from_json(value)?.with_status(status))
}
