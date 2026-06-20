use crate::models::{
    BookmarkPayload, BulkBookmarksPayload, FolderPayload, HealthResponse, IdsPayload,
    MoveBookmarksPayload, RenameFolderPayload, ReorderBookmarksPayload, ReorderFoldersPayload,
    WebdavConfigPayload,
};
use crate::{db, export};
use worker::d1::D1Database;
use worker::*;

pub async fn handle(req: Request, env: Env) -> Result<Response> {
    Router::new()
        .get_async("/api/health", |_req, _ctx| async move {
            Response::from_json(&HealthResponse::success())
        })
        .get_async("/api/bookmarks", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            Response::from_json(&db::all_bookmarks(&db).await?)
        })
        .get_async("/api/bootstrap", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            Response::from_json(&db::bootstrap_data(&db).await?)
        })
        .post_async("/api/bookmarks", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<BookmarkPayload>().await.unwrap_or_default();

            match db::save_bookmark(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/bulk", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<BulkBookmarksPayload>().await.unwrap_or_default();

            match db::bulk_save_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/move", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<MoveBookmarksPayload>().await.unwrap_or_default();

            match db::move_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/reorder", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req
                .json::<ReorderBookmarksPayload>()
                .await
                .unwrap_or_default();
            Response::from_json(&db::reorder_bookmarks(&db, payload).await?)
        })
        .post_async("/api/bookmarks/delete", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<IdsPayload>().await.unwrap_or_default();

            match db::delete_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .delete_async("/api/bookmarks/:id", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let id = ctx.param("id").map(String::as_str).unwrap_or("");
            Response::from_json(&db::delete_bookmark(&db, id).await?)
        })
        .get_async("/api/folder-orders", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            Response::from_json(&db::all_folder_orders(&db).await?)
        })
        .post_async("/api/folders/reorder", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req
                .json::<ReorderFoldersPayload>()
                .await
                .unwrap_or_default();
            Response::from_json(&db::reorder_folders(&db, payload).await?)
        })
        .post_async("/api/folders/move-up", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<FolderPayload>().await.unwrap_or_default();

            match db::move_folder_up(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/folders/rename", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<RenameFolderPayload>().await.unwrap_or_default();

            match db::rename_folder(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/folders/delete", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let payload = req.json::<FolderPayload>().await.unwrap_or_default();

            match db::delete_folder(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .get_async("/api/bookmarks/export", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let bookmarks = db::all_bookmarks(&db).await?;
            let timestamp = (js_sys::Date::now() / 1000.0).floor() as i64;
            let html = export::build_bookmarks_html(&bookmarks, timestamp);
            let headers = Headers::new();
            headers.set(
                "Content-Disposition",
                &format!(
                    r#"attachment; filename="{}""#,
                    export::current_export_filename()
                ),
            )?;
            Ok(Response::from_html(html)?.with_headers(headers))
        })
        .get_async("/api/webdav/config", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            Response::from_json(&db::webdav_config(&db).await?)
        })
        .post_async("/api/webdav/config", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let secret = ctx
                .env
                .secret(crate::crypto::SECRET_BINDING)
                .ok()
                .map(|value| value.to_string());
            let payload = req.json::<WebdavConfigPayload>().await.unwrap_or_default();

            match db::update_webdav_config(&db, payload, secret).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .run(req, env)
        .await
}

fn json_with_status(value: &serde_json::Value, status: u16) -> Result<Response> {
    Ok(Response::from_json(value)?.with_status(status))
}

async fn initialized_db(env: &Env) -> Result<D1Database> {
    let db = env.d1(db::D1_BINDING)?;
    db::initialize_schema(&db).await?;
    Ok(db)
}
