use crate::models::{
    BookmarkPayload, BulkBookmarksPayload, ErrorResponse, FolderPayload, HealthResponse,
    IdsPayload, MoveBookmarksPayload, RenameFolderPayload, ReorderBookmarksPayload,
    ReorderFoldersPayload, WebdavConfigPayload,
};
use crate::{auth, db, export, webauthn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::d1::D1Database;
use worker::*;

const AUTH_CHALLENGE_TTL_SECONDS: i64 = 5 * 60;

pub async fn handle(req: Request, env: Env) -> Result<Response> {
    Router::new()
        .get_async("/api/health", |_req, _ctx| async move {
            Response::from_json(&HealthResponse::success())
        })
        .get_async("/api/auth/status", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let auth_config = auth::auth_config(&ctx.env, &req)?;
            let admin_initialized = auth::is_setup_completed(&db).await?
                || auth::count_admin_credentials(&db).await? > 0;
            let session = auth::require_admin_session(&db, &req).await.ok();

            Response::from_json(&AuthStatusResponse {
                public_read: true,
                admin_initialized,
                admin_unlocked: session.is_some(),
                admin_session_expires_at: session.map(|session| session.expires_at),
                auth_configured: auth_config.configured,
                missing_config: auth_config.missing_config,
            })
        })
        .post_async(
            "/api/auth/passkey/register/options",
            |mut req, ctx| async move {
                let db = initialized_db(&ctx.env).await?;
                let payload = req
                    .json::<PasskeyRegisterOptionsPayload>()
                    .await
                    .unwrap_or_default();

                match passkey_register_options(&db, &req, &ctx.env, payload).await {
                    Ok(Ok(response)) => Response::from_json(&response),
                    Ok(Err((status, body))) => json_with_status(&body, status),
                    Err(error) => Err(error),
                }
            },
        )
        .post_async(
            "/api/auth/passkey/register/verify",
            |mut req, ctx| async move {
                let db = initialized_db(&ctx.env).await?;
                let payload = req
                    .json::<PasskeyRegisterVerifyPayload>()
                    .await
                    .unwrap_or_default();

                match passkey_register_verify(&db, &req, &ctx.env, payload).await {
                    Ok(Ok((response, cookie))) => json_with_cookie(&response, &cookie),
                    Ok(Err((status, body))) => json_with_status(&body, status),
                    Err(error) => Err(error),
                }
            },
        )
        .post_async("/api/auth/passkey/login/options", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;

            match passkey_login_options(&db, &req, &ctx.env).await {
                Ok(Ok(response)) => Response::from_json(&response),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
        })
        .post_async(
            "/api/auth/passkey/login/verify",
            |mut req, ctx| async move {
                let db = initialized_db(&ctx.env).await?;
                let payload = req
                    .json::<PasskeyLoginVerifyPayload>()
                    .await
                    .unwrap_or_default();

                match passkey_login_verify(&db, &req, &ctx.env, payload).await {
                    Ok(Ok((response, cookie))) => json_with_cookie(&response, &cookie),
                    Ok(Err((status, body))) => json_with_status(&body, status),
                    Err(error) => Err(error),
                }
            },
        )
        .post_async("/api/auth/logout", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let config = auth::auth_config(&ctx.env, &req)?;

            if let Ok(session) = auth::require_admin_session(&db, &req).await {
                auth::revoke_admin_session(&db, &session.id, auth::now_timestamp()).await?;
            }

            json_with_cookie(
                &json!({"ok": true, "admin_unlocked": false}),
                &auth::clear_session_cookie(&config),
            )
        })
        .get_async("/api/auth/passkeys", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            match passkey_list(&db, &req).await {
                Ok(Ok(response)) => Response::from_json(&response),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
        })
        .delete_async("/api/auth/passkeys/:credential_id", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let credential_id = ctx.param("credential_id").map(String::as_str).unwrap_or("");
            match passkey_delete(&db, &req, &ctx.env, credential_id).await {
                Ok(Ok(response)) => Response::from_json(&response),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
        })
        .get_async("/api/auth/sessions", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            match session_list(&db, &req).await {
                Ok(Ok(response)) => Response::from_json(&response),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
        })
        .delete_async("/api/auth/sessions/:session_id", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            let session_id = ctx.param("session_id").map(String::as_str).unwrap_or("");
            match session_revoke(&db, &req, &ctx.env, session_id).await {
                Ok(Ok((response, Some(cookie)))) => json_with_cookie(&response, &cookie),
                Ok(Ok((response, None))) => Response::from_json(&response),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
        })
        .post_async("/api/auth/sessions/revoke-all", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            match session_revoke_all(&db, &req, &ctx.env).await {
                Ok(Ok((response, cookie))) => json_with_cookie(&response, &cookie),
                Ok(Err((status, body))) => json_with_status(&body, status),
                Err(error) => Err(error),
            }
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
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<BookmarkPayload>().await.unwrap_or_default();

            match db::save_bookmark(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/bulk", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<BulkBookmarksPayload>().await.unwrap_or_default();

            match db::bulk_save_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/move", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<MoveBookmarksPayload>().await.unwrap_or_default();

            match db::move_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/bookmarks/reorder", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req
                .json::<ReorderBookmarksPayload>()
                .await
                .unwrap_or_default();
            Response::from_json(&db::reorder_bookmarks(&db, payload).await?)
        })
        .post_async("/api/bookmarks/delete", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<IdsPayload>().await.unwrap_or_default();

            match db::delete_bookmarks(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .delete_async("/api/bookmarks/:id", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_state_change(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let id = ctx.param("id").map(String::as_str).unwrap_or("");
            Response::from_json(&db::delete_bookmark(&db, id).await?)
        })
        .get_async("/api/folder-orders", |_req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            Response::from_json(&db::all_folder_orders(&db).await?)
        })
        .post_async("/api/folders/reorder", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req
                .json::<ReorderFoldersPayload>()
                .await
                .unwrap_or_default();
            Response::from_json(&db::reorder_folders(&db, payload).await?)
        })
        .post_async("/api/folders/move-up", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<FolderPayload>().await.unwrap_or_default();

            match db::move_folder_up(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/folders/rename", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

            let payload = req.json::<RenameFolderPayload>().await.unwrap_or_default();

            match db::rename_folder(&db, payload).await? {
                Ok(response) => Response::from_json(&response),
                Err((status, body)) => json_with_status(&body, status),
            }
        })
        .post_async("/api/folders/delete", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

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
        .get_async("/api/webdav/config", |req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_session_only(&db, &req).await? {
                return json_with_status(&body, status);
            }

            Response::from_json(&db::webdav_config(&db).await?)
        })
        .post_async("/api/webdav/config", |mut req, ctx| async move {
            let db = initialized_db(&ctx.env).await?;
            if let Err((status, body)) = require_admin_json_request(&db, &req, &ctx.env).await? {
                return json_with_status(&body, status);
            }

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

fn json_with_cookie(value: &serde_json::Value, cookie: &str) -> Result<Response> {
    let headers = Headers::new();
    auth::add_set_cookie_header(&headers, cookie)?;
    Ok(Response::from_json(value)?.with_headers(headers))
}

fn auth_error(_status: u16, error: &'static str, message: impl Into<String>) -> serde_json::Value {
    serde_json::to_value(ErrorResponse::with_code(error, message)).unwrap_or_else(|_| {
        json!({
            "status": "error",
            "error": error,
            "message": "认证失败"
        })
    })
}

fn guard_error(error: auth::AuthGuardError) -> (u16, serde_json::Value) {
    (
        error.status(),
        auth_error(error.status(), error.code(), error.message()),
    )
}

async fn require_admin_session_only(
    db: &D1Database,
    req: &Request,
) -> Result<Result<(), (u16, serde_json::Value)>> {
    match auth::require_admin_session(db, req).await {
        Ok(_) => Ok(Ok(())),
        Err(error) => Ok(Err(guard_error(error))),
    }
}

async fn require_admin_json_request(
    db: &D1Database,
    req: &Request,
    env: &Env,
) -> Result<Result<(), (u16, serde_json::Value)>> {
    match auth::require_admin_request(db, req, env).await {
        Ok(_) => Ok(Ok(())),
        Err(error) => Ok(Err(guard_error(error))),
    }
}

async fn require_admin_state_change(
    db: &D1Database,
    req: &Request,
    env: &Env,
) -> Result<Result<(), (u16, serde_json::Value)>> {
    let config = auth::auth_config(env, req)?;

    if !config.configured {
        return Ok(Err(guard_error(auth::AuthGuardError::AuthConfigRequired(
            config.missing_config,
        ))));
    }

    if let Err(error) = auth::ensure_same_origin(req, &config) {
        return Ok(Err(guard_error(error)));
    }

    require_admin_session_only(db, req).await
}

async fn ensure_auth_post_request(
    req: &Request,
    env: &Env,
) -> Result<Result<auth::AuthConfig, (u16, serde_json::Value)>> {
    let config = auth::auth_config(env, req)?;

    if !config.configured {
        return Ok(Err(guard_error(auth::AuthGuardError::AuthConfigRequired(
            config.missing_config,
        ))));
    }

    if let Err(error) = auth::ensure_json_request(req) {
        return Ok(Err(guard_error(error)));
    }

    if let Err(error) = auth::ensure_same_origin(req, &config) {
        return Ok(Err(guard_error(error)));
    }

    Ok(Ok(config))
}

async fn passkey_register_options(
    db: &D1Database,
    req: &Request,
    env: &Env,
    payload: PasskeyRegisterOptionsPayload,
) -> Result<Result<serde_json::Value, (u16, serde_json::Value)>> {
    let config = match ensure_auth_post_request(req, env).await? {
        Ok(config) => config,
        Err(error) => return Ok(Err(error)),
    };
    let now = auth::now_timestamp();
    let setup_allowed = auth::can_use_setup_token(db).await?;

    if setup_allowed {
        let setup_token = env
            .secret(auth::SETUP_TOKEN_BINDING)
            .ok()
            .map(|value| value.to_string())
            .unwrap_or_default();

        if setup_token.is_empty()
            || payload
                .setup_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                != Some(setup_token.as_str())
        {
            return Ok(Err((
                403,
                auth_error(403, "auth_required", "无法初始化管理权限"),
            )));
        }
    } else if let Err(error) = auth::require_admin_session(db, req).await {
        return Ok(Err(guard_error(error)));
    }

    let challenge = auth::random_base64url(32)?;
    auth::insert_auth_challenge(
        db,
        &auth::NewAuthChallenge {
            id: challenge.clone(),
            challenge: challenge.clone(),
            purpose: auth::PURPOSE_PASSKEY_REGISTRATION.to_string(),
            created_at: now,
            expires_at: now + AUTH_CHALLENGE_TTL_SECONDS,
        },
    )
    .await?;

    let credentials = auth::list_admin_credentials(db).await?;
    let exclude_credentials = credentials
        .into_iter()
        .map(|credential| {
            json!({
                "type": "public-key",
                "id": credential.credential_id
            })
        })
        .collect::<Vec<_>>();
    let name = payload
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Linkwise Admin");

    Ok(Ok(json!({
        "publicKey": {
            "challenge": challenge,
            "rp": {
                "name": "Linkwise",
                "id": config.rp_id
            },
            "user": {
                "id": auth::random_base64url(16)?,
                "name": "admin",
                "displayName": name
            },
            "pubKeyCredParams": [
                {"type": "public-key", "alg": -7}
            ],
            "timeout": 300000,
            "attestation": "none",
            "authenticatorSelection": {
                "residentKey": "preferred",
                "userVerification": "preferred"
            },
            "excludeCredentials": exclude_credentials
        }
    })))
}

async fn passkey_register_verify(
    db: &D1Database,
    req: &Request,
    env: &Env,
    payload: PasskeyRegisterVerifyPayload,
) -> Result<Result<(serde_json::Value, String), (u16, serde_json::Value)>> {
    let config = match ensure_auth_post_request(req, env).await? {
        Ok(config) => config,
        Err(error) => return Ok(Err(error)),
    };
    let setup_allowed = auth::can_use_setup_token(db).await?;

    if !setup_allowed {
        if let Err(error) = auth::require_admin_session(db, req).await {
            return Ok(Err(guard_error(error)));
        }
    }

    let verification = match webauthn::verify_registration_payload(
        &payload.credential,
        &config.origin,
        &config.rp_id,
    ) {
        Ok(verification) => verification,
        Err(_) => {
            return Ok(Err((
                400,
                auth_error(400, "invalid_webauthn_response", "Passkey 注册验证失败"),
            )))
        }
    };
    let now = auth::now_timestamp();
    let challenge = auth::get_valid_auth_challenge(
        db,
        &verification.challenge,
        auth::PURPOSE_PASSKEY_REGISTRATION,
        now,
    )
    .await?;

    if challenge.is_none() {
        return Ok(Err((
            400,
            auth_error(400, "invalid_request", "Passkey 注册挑战无效或已过期"),
        )));
    }

    let name = payload
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Passkey")
        .to_string();

    auth::insert_admin_credential(
        db,
        &auth::NewAdminCredential {
            credential_id: verification.credential_id.clone(),
            public_key: verification.public_key_jwk,
            sign_count: verification.sign_count,
            name,
            created_at: now,
        },
    )
    .await?;
    auth::mark_auth_challenge_used(db, &verification.challenge, now).await?;

    if setup_allowed {
        auth::mark_setup_completed(db, now).await?;
    }

    let (expires_at, cookie) = create_admin_session_cookie(
        db,
        &config,
        Some(verification.credential_id),
        payload.session_max_age_seconds,
        now,
    )
    .await?;

    Ok(Ok((
        json!({
            "ok": true,
            "admin_initialized": true,
            "admin_unlocked": true,
            "expires_at": expires_at
        }),
        cookie,
    )))
}

async fn passkey_login_options(
    db: &D1Database,
    req: &Request,
    env: &Env,
) -> Result<Result<serde_json::Value, (u16, serde_json::Value)>> {
    let config = match ensure_auth_post_request(req, env).await? {
        Ok(config) => config,
        Err(error) => return Ok(Err(error)),
    };

    if auth::count_admin_credentials(db).await? == 0 {
        return Ok(Err((
            403,
            auth_error(403, "auth_required", "需要先初始化管理权限"),
        )));
    }

    let now = auth::now_timestamp();
    let challenge = auth::random_base64url(32)?;
    auth::insert_auth_challenge(
        db,
        &auth::NewAuthChallenge {
            id: challenge.clone(),
            challenge: challenge.clone(),
            purpose: auth::PURPOSE_PASSKEY_LOGIN.to_string(),
            created_at: now,
            expires_at: now + AUTH_CHALLENGE_TTL_SECONDS,
        },
    )
    .await?;

    let allow_credentials = auth::list_admin_credentials(db)
        .await?
        .into_iter()
        .map(|credential| json!({"type": "public-key", "id": credential.credential_id}))
        .collect::<Vec<_>>();

    Ok(Ok(json!({
        "publicKey": {
            "challenge": challenge,
            "rpId": config.rp_id,
            "allowCredentials": allow_credentials,
            "timeout": 300000,
            "userVerification": "preferred"
        }
    })))
}

async fn passkey_login_verify(
    db: &D1Database,
    req: &Request,
    env: &Env,
    payload: PasskeyLoginVerifyPayload,
) -> Result<Result<(serde_json::Value, String), (u16, serde_json::Value)>> {
    let config = match ensure_auth_post_request(req, env).await? {
        Ok(config) => config,
        Err(error) => return Ok(Err(error)),
    };
    let credential_id = match webauthn::credential_id_from_payload(&payload.credential) {
        Ok(credential_id) => credential_id,
        Err(_) => {
            return Ok(Err((
                400,
                auth_error(400, "invalid_webauthn_response", "Passkey 登录验证失败"),
            )))
        }
    };
    let Some(credential) = auth::get_admin_credential(db, &credential_id).await? else {
        return Ok(Err((
            403,
            auth_error(403, "auth_required", "Passkey 不存在"),
        )));
    };
    let verification = match webauthn::verify_login_payload(
        &payload.credential,
        &config.origin,
        &config.rp_id,
        &credential.public_key,
    )
    .await
    {
        Ok(verification) => verification,
        Err(_) => {
            return Ok(Err((
                400,
                auth_error(400, "invalid_webauthn_response", "Passkey 登录验证失败"),
            )))
        }
    };
    let now = auth::now_timestamp();
    let challenge = auth::get_valid_auth_challenge(
        db,
        &verification.challenge,
        auth::PURPOSE_PASSKEY_LOGIN,
        now,
    )
    .await?;

    if challenge.is_none() || verification.credential_id != credential_id {
        return Ok(Err((
            400,
            auth_error(400, "invalid_request", "Passkey 登录挑战无效或已过期"),
        )));
    }

    auth::mark_auth_challenge_used(db, &verification.challenge, now).await?;
    auth::update_admin_credential_usage(
        db,
        &credential_id,
        verification.sign_count.max(credential.sign_count),
        now,
    )
    .await?;

    let (expires_at, cookie) = create_admin_session_cookie(
        db,
        &config,
        Some(credential_id),
        payload.session_max_age_seconds,
        now,
    )
    .await?;

    Ok(Ok((
        json!({
            "ok": true,
            "admin_unlocked": true,
            "expires_at": expires_at
        }),
        cookie,
    )))
}

async fn passkey_list(
    db: &D1Database,
    req: &Request,
) -> Result<Result<serde_json::Value, (u16, serde_json::Value)>> {
    if let Err(error) = auth::require_admin_session(db, req).await {
        return Ok(Err(guard_error(error)));
    }

    let passkeys = auth::list_admin_credentials(db)
        .await?
        .into_iter()
        .map(|credential| {
            json!({
                "credential_id": credential.credential_id,
                "name": credential.name,
                "created_at": credential.created_at,
                "last_used_at": credential.last_used_at
            })
        })
        .collect::<Vec<_>>();

    Ok(Ok(json!({
        "ok": true,
        "passkeys": passkeys
    })))
}

async fn passkey_delete(
    db: &D1Database,
    req: &Request,
    env: &Env,
    credential_id: &str,
) -> Result<Result<serde_json::Value, (u16, serde_json::Value)>> {
    let config = auth::auth_config(env, req)?;

    if let Err(error) = auth::ensure_same_origin(req, &config) {
        return Ok(Err(guard_error(error)));
    }

    if let Err(error) = auth::require_admin_session(db, req).await {
        return Ok(Err(guard_error(error)));
    }

    if credential_id.is_empty()
        || auth::get_admin_credential(db, credential_id)
            .await?
            .is_none()
    {
        return Ok(Err((404, auth_error(404, "not_found", "Passkey 不存在"))));
    }

    if auth::count_admin_credentials(db).await? <= 1 {
        return Ok(Err((
            403,
            auth_error(403, "setup_not_allowed", "不能删除最后一个 Passkey"),
        )));
    }

    let now = auth::now_timestamp();
    auth::delete_admin_credential(db, credential_id).await?;
    auth::revoke_sessions_for_credential(db, credential_id, now).await?;

    Ok(Ok(json!({
        "ok": true,
        "deleted_credential_id": credential_id
    })))
}

async fn session_list(
    db: &D1Database,
    req: &Request,
) -> Result<Result<serde_json::Value, (u16, serde_json::Value)>> {
    let current_session = match auth::require_admin_session(db, req).await {
        Ok(session) => session,
        Err(error) => return Ok(Err(guard_error(error))),
    };
    let credentials = auth::list_admin_credentials(db).await?;
    let credential_names = credentials
        .into_iter()
        .map(|credential| (credential.credential_id, credential.name))
        .collect::<std::collections::HashMap<_, _>>();
    let sessions = auth::list_admin_sessions(db)
        .await?
        .into_iter()
        .map(|session| {
            let credential_name = session
                .credential_id
                .as_ref()
                .and_then(|credential_id| credential_names.get(credential_id))
                .cloned();

            json!({
                "id": session.id,
                "credential_id": session.credential_id,
                "credential_name": credential_name,
                "created_at": session.created_at,
                "last_seen_at": session.last_seen_at,
                "expires_at": session.expires_at,
                "revoked_at": session.revoked_at,
                "current": session.id == current_session.id
            })
        })
        .collect::<Vec<_>>();

    Ok(Ok(json!({
        "ok": true,
        "sessions": sessions
    })))
}

async fn session_revoke(
    db: &D1Database,
    req: &Request,
    env: &Env,
    session_id: &str,
) -> Result<Result<(serde_json::Value, Option<String>), (u16, serde_json::Value)>> {
    let config = auth::auth_config(env, req)?;

    if let Err(error) = auth::ensure_same_origin(req, &config) {
        return Ok(Err(guard_error(error)));
    }

    let current_session = match auth::require_admin_session(db, req).await {
        Ok(session) => session,
        Err(error) => return Ok(Err(guard_error(error))),
    };

    if session_id.is_empty() {
        return Ok(Err((
            400,
            auth_error(400, "invalid_request", "Session ID 无效"),
        )));
    }

    auth::revoke_admin_session(db, session_id, auth::now_timestamp()).await?;
    let revoked_current = session_id == current_session.id;
    let cookie = revoked_current.then(|| auth::clear_session_cookie(&config));

    Ok(Ok((
        json!({
            "ok": true,
            "revoked_session_id": session_id,
            "admin_unlocked": !revoked_current
        }),
        cookie,
    )))
}

async fn session_revoke_all(
    db: &D1Database,
    req: &Request,
    env: &Env,
) -> Result<Result<(serde_json::Value, String), (u16, serde_json::Value)>> {
    let config = auth::auth_config(env, req)?;

    if let Err(error) = auth::ensure_same_origin(req, &config) {
        return Ok(Err(guard_error(error)));
    }

    if let Err(error) = auth::require_admin_session(db, req).await {
        return Ok(Err(guard_error(error)));
    }

    auth::revoke_all_admin_sessions(db, auth::now_timestamp()).await?;

    Ok(Ok((
        json!({
            "ok": true,
            "admin_unlocked": false
        }),
        auth::clear_session_cookie(&config),
    )))
}

async fn create_admin_session_cookie(
    db: &D1Database,
    config: &auth::AuthConfig,
    credential_id: Option<String>,
    requested_max_age: Option<i64>,
    now: i64,
) -> Result<(i64, String)> {
    let token = auth::random_base64url(32)?;
    let token_hash = auth::hash_session_token(&token);
    let session_id = auth::random_base64url(18)?;
    let max_age = requested_max_age
        .filter(|value| *value > 0)
        .map(|value| value.min(auth::WEB_SESSION_MAX_AGE_SECONDS));
    let expires_at = auth::session_expires_at(now, max_age);

    auth::insert_admin_session(
        db,
        &auth::NewAdminSession {
            id: session_id,
            token_hash,
            credential_id,
            created_at: now,
            last_seen_at: now,
            expires_at,
        },
    )
    .await?;

    Ok((
        expires_at,
        auth::build_session_cookie(&token, config, max_age),
    ))
}

async fn initialized_db(env: &Env) -> Result<D1Database> {
    let db = env.d1(db::D1_BINDING)?;
    db::initialize_schema(&db).await?;
    Ok(db)
}

#[derive(Serialize)]
struct AuthStatusResponse {
    public_read: bool,
    admin_initialized: bool,
    admin_unlocked: bool,
    admin_session_expires_at: Option<i64>,
    auth_configured: bool,
    missing_config: Vec<&'static str>,
}

#[derive(Debug, Default, Deserialize)]
struct PasskeyRegisterOptionsPayload {
    setup_token: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PasskeyRegisterVerifyPayload {
    name: Option<String>,
    session_max_age_seconds: Option<i64>,
    credential: webauthn::PublicKeyCredentialPayload,
}

#[derive(Debug, Default, Deserialize)]
struct PasskeyLoginVerifyPayload {
    session_max_age_seconds: Option<i64>,
    credential: webauthn::PublicKeyCredentialPayload,
}
