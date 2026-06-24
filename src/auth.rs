#![allow(dead_code)]

use crate::crypto;
use crate::models::{AdminCredential, AdminSession, AppDeviceSession, AuthChallenge, CountValue};
use js_sys::wasm_bindgen::JsCast;
use js_sys::{Reflect, Uint8Array};
use worker::d1::{D1Database, D1Type};
use worker::*;

pub const ADMIN_SESSION_COOKIE: &str = "linkwise_admin_session";
pub const AUTH_ORIGIN_BINDING: &str = "LINKWISE_AUTH_ORIGIN";
pub const AUTH_RP_ID_BINDING: &str = "LINKWISE_AUTH_RP_ID";
pub const SETUP_TOKEN_BINDING: &str = "LINKWISE_SETUP_TOKEN";
pub const SETUP_COMPLETED_KEY: &str = "auth.setup_completed";
pub const SETUP_COMPLETED_AT_KEY: &str = "auth.setup_completed_at";
pub const PURPOSE_PASSKEY_REGISTRATION: &str = "passkey_registration";
pub const PURPOSE_PASSKEY_LOGIN: &str = "passkey_login";
pub const WEB_SESSION_MAX_AGE_SECONDS: i64 = 24 * 60 * 60;
pub const AUTH_RATE_LIMIT_MAX_FAILURES: i64 = 5;
pub const AUTH_RATE_LIMIT_WINDOW_SECONDS: i64 = 5 * 60;
pub const AUTH_RATE_LIMIT_LOCK_SECONDS: i64 = 5 * 60;
pub const APP_DEVICE_TOKEN_PREFIX: &str = "lwapp_";
pub const APP_DEVICE_TOKEN_RANDOM_BYTES: u32 = 32;
pub const APP_DEVICE_TOKEN_PREFIX_CHARS: usize = APP_DEVICE_TOKEN_PREFIX.len() + 8;

#[derive(Debug, Clone)]
pub struct NewAdminCredential {
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: i64,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewAuthChallenge {
    pub id: String,
    pub challenge: String,
    pub purpose: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewAdminSession {
    pub id: String,
    pub token_hash: String,
    pub credential_id: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewAppDeviceSession {
    pub id: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub name: String,
    pub issued_by_credential_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthRateLimit {
    pub bucket: String,
    pub failed_count: i64,
    pub first_failed_at: i64,
    pub last_failed_at: i64,
    pub locked_until: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub origin: String,
    pub rp_id: String,
    pub is_local_dev: bool,
    pub configured: bool,
    pub missing_config: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub enum AuthGuardError {
    AdminSessionRequired,
    AppSessionRequired,
    AuthConfigRequired(Vec<&'static str>),
    InvalidContentType,
    InvalidOrigin,
    MixedAuthNotAllowed,
}

impl AuthGuardError {
    pub fn status(&self) -> u16 {
        match self {
            Self::AdminSessionRequired | Self::AppSessionRequired => 401,
            Self::InvalidContentType | Self::MixedAuthNotAllowed => 400,
            Self::AuthConfigRequired(_) | Self::InvalidOrigin => 403,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::AdminSessionRequired => "admin_session_required",
            Self::AppSessionRequired => "app_session_required",
            Self::AuthConfigRequired(_) => "auth_config_required",
            Self::InvalidContentType => "invalid_content_type",
            Self::InvalidOrigin => "invalid_origin",
            Self::MixedAuthNotAllowed => "mixed_auth_not_allowed",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::AdminSessionRequired => "需要解锁管理模式",
            Self::AppSessionRequired => "需要有效 App 设备会话",
            Self::AuthConfigRequired(_) => "认证配置不完整",
            Self::InvalidContentType => "请求类型无效",
            Self::InvalidOrigin => "请求来源无效",
            Self::MixedAuthNotAllowed => "请求不能同时使用 Web 会话和 App Token",
        }
    }
}

pub fn now_timestamp() -> i64 {
    (js_sys::Date::now() / 1000.0).floor() as i64
}

pub fn hash_session_token(token: &str) -> String {
    crypto::sha256_hex(token)
}

pub fn app_device_token_prefix(token: &str) -> String {
    token.chars().take(APP_DEVICE_TOKEN_PREFIX_CHARS).collect()
}

pub fn build_app_device_token() -> Result<String> {
    Ok(format!(
        "{APP_DEVICE_TOKEN_PREFIX}{}",
        random_base64url(APP_DEVICE_TOKEN_RANDOM_BYTES)?
    ))
}

pub fn session_expires_at(now: i64, requested_max_age: Option<i64>) -> i64 {
    let max_age = requested_max_age
        .filter(|value| *value > 0)
        .unwrap_or(WEB_SESSION_MAX_AGE_SECONDS)
        .min(WEB_SESSION_MAX_AGE_SECONDS);
    now + max_age
}

pub fn random_base64url(byte_len: u32) -> Result<String> {
    let bytes = random_bytes(byte_len)?;
    Ok(base64url_encode(&bytes))
}

pub fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut index = 0;

    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = bytes.get(index + 1).copied();
        let b2 = bytes.get(index + 2).copied();

        output.push(ALPHABET[(b0 >> 2) as usize] as char);
        output.push(ALPHABET[(((b0 & 0x03) << 4) | b1.unwrap_or(0) >> 4) as usize] as char);

        if let Some(b1) = b1 {
            output.push(ALPHABET[(((b1 & 0x0f) << 2) | b2.unwrap_or(0) >> 6) as usize] as char);
        }

        if let Some(b2) = b2 {
            output.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        }

        index += 3;
    }

    output
}

pub fn base64url_decode(value: &str) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bit_count = 0u8;

    for byte in value.bytes() {
        let Some(bits) = base64url_value(byte) else {
            if byte == b'=' {
                break;
            }

            return Err(Error::RustError("invalid base64url input".to_string()));
        };

        buffer = (buffer << 6) | bits as u32;
        bit_count += 6;

        while bit_count >= 8 {
            bit_count -= 8;
            output.push((buffer >> bit_count) as u8);
            buffer &= (1 << bit_count) - 1;
        }
    }

    Ok(output)
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn random_bytes(byte_len: u32) -> Result<Vec<u8>> {
    let global = js_sys::global();
    let crypto = Reflect::get(&global, &"crypto".into())
        .map_err(|_| Error::RustError("crypto global is unavailable".to_string()))?;
    let get_random_values = Reflect::get(&crypto, &"getRandomValues".into())
        .map_err(|_| Error::RustError("crypto.getRandomValues is unavailable".to_string()))?;
    let get_random_values = get_random_values
        .dyn_into::<js_sys::Function>()
        .map_err(|_| Error::RustError("crypto.getRandomValues is not callable".to_string()))?;
    let array = Uint8Array::new_with_length(byte_len);

    get_random_values
        .call1(&crypto, &array)
        .map_err(|_| Error::RustError("crypto.getRandomValues failed".to_string()))?;

    Ok(array.to_vec())
}

pub fn optional_env_var(env: &Env, binding: &str) -> Option<String> {
    env.var(binding)
        .ok()
        .map(|value| value.to_string())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn request_origin(req: &Request) -> Result<String> {
    let url = req.url()?;
    let scheme = url.scheme();
    let host = url
        .host_str()
        .ok_or_else(|| Error::RustError("request URL is missing host".to_string()))?;

    match url.port() {
        Some(port) => Ok(format!("{scheme}://{host}:{port}")),
        None => Ok(format!("{scheme}://{host}")),
    }
}

pub fn auth_config(env: &Env, req: &Request) -> Result<AuthConfig> {
    let request_origin = request_origin(req)?;
    let request_host = req.url()?.host_str().unwrap_or_default().to_string();
    let is_local_dev = is_local_host(&request_host);
    let origin = optional_env_var(env, AUTH_ORIGIN_BINDING);
    let rp_id = optional_env_var(env, AUTH_RP_ID_BINDING);
    let mut missing_config = Vec::new();

    if origin.is_none() && !is_local_dev {
        missing_config.push(AUTH_ORIGIN_BINDING);
    }

    if rp_id.is_none() && !is_local_dev {
        missing_config.push(AUTH_RP_ID_BINDING);
    }

    Ok(AuthConfig {
        origin: origin.unwrap_or_else(|| request_origin.clone()),
        rp_id: rp_id.unwrap_or_else(|| local_rp_id(&request_host)),
        is_local_dev,
        configured: missing_config.is_empty(),
        missing_config,
    })
}

pub fn should_use_secure_cookie(config: &AuthConfig) -> bool {
    config.origin.starts_with("https://")
}

pub fn build_session_cookie(token: &str, config: &AuthConfig, max_age: Option<i64>) -> String {
    let mut parts = vec![
        format!("{ADMIN_SESSION_COOKIE}={token}"),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
        "Path=/".to_string(),
    ];

    if should_use_secure_cookie(config) {
        parts.push("Secure".to_string());
    }

    if let Some(max_age) = max_age.filter(|value| *value > 0) {
        parts.push(format!(
            "Max-Age={}",
            max_age.min(WEB_SESSION_MAX_AGE_SECONDS)
        ));
    }

    parts.join("; ")
}

pub fn clear_session_cookie(config: &AuthConfig) -> String {
    let mut parts = vec![
        format!("{ADMIN_SESSION_COOKIE}="),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
        "Path=/".to_string(),
        "Max-Age=0".to_string(),
    ];

    if should_use_secure_cookie(config) {
        parts.push("Secure".to_string());
    }

    parts.join("; ")
}

pub fn add_set_cookie_header(headers: &Headers, cookie: &str) -> Result<()> {
    headers.append("Set-Cookie", cookie)
}

pub fn session_token_from_request(req: &Request) -> Result<Option<String>> {
    let Some(cookie_header) = req.headers().get("Cookie")? else {
        return Ok(None);
    };

    Ok(parse_cookie_value(&cookie_header, ADMIN_SESSION_COOKIE))
}

pub fn authorization_bearer_token_from_request(req: &Request) -> Result<Option<String>> {
    let Some(authorization) = req.headers().get("Authorization")? else {
        return Ok(None);
    };
    let mut pieces = authorization.trim().splitn(2, ' ');
    let scheme = pieces.next().unwrap_or_default();
    let token = pieces.next().unwrap_or_default().trim();

    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        return Ok(Some(token.to_string()));
    }

    Ok(None)
}

pub fn app_device_token_from_request(req: &Request) -> Result<Option<String>> {
    Ok(authorization_bearer_token_from_request(req)?
        .filter(|token| token.starts_with(APP_DEVICE_TOKEN_PREFIX)))
}

pub fn parse_cookie_value(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let mut pieces = part.trim().splitn(2, '=');
        let Some(cookie_name) = pieces.next() else {
            continue;
        };
        let Some(cookie_value) = pieces.next() else {
            continue;
        };

        if cookie_name == name {
            return Some(cookie_value.to_string()).filter(|value| !value.is_empty());
        }
    }

    None
}

pub fn setup_rate_limit_bucket(req: &Request) -> String {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")
        .ok()
        .flatten()
        .or_else(|| req.headers().get("X-Forwarded-For").ok().flatten())
        .and_then(|value| value.split(',').next().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    format!("setup:{ip}")
}

pub fn is_unsafe_method(method: &Method) -> bool {
    !matches!(method, Method::Get | Method::Head | Method::Options)
}

pub fn ensure_json_request(req: &Request) -> std::result::Result<(), AuthGuardError> {
    if !is_unsafe_method(&req.method()) {
        return Ok(());
    }

    let content_type = req
        .headers()
        .get("Content-Type")
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if content_type
        .split(';')
        .next()
        .map(str::trim)
        .is_some_and(|value| value == "application/json")
    {
        return Ok(());
    }

    Err(AuthGuardError::InvalidContentType)
}

pub fn ensure_same_origin(
    req: &Request,
    config: &AuthConfig,
) -> std::result::Result<(), AuthGuardError> {
    if !is_unsafe_method(&req.method()) {
        return Ok(());
    }

    let origin = req.headers().get("Origin").ok().flatten();

    match origin {
        Some(origin) if origin == config.origin => Ok(()),
        None if config.is_local_dev => Ok(()),
        _ => Err(AuthGuardError::InvalidOrigin),
    }
}

pub async fn require_admin_session(
    db: &D1Database,
    req: &Request,
) -> std::result::Result<AdminSession, AuthGuardError> {
    let token = session_token_from_request(req)
        .map_err(|_| AuthGuardError::AdminSessionRequired)?
        .ok_or(AuthGuardError::AdminSessionRequired)?;
    let token_hash = hash_session_token(&token);
    let now = now_timestamp();
    let session = get_valid_admin_session_by_hash(db, &token_hash, now)
        .await
        .map_err(|_| AuthGuardError::AdminSessionRequired)?
        .ok_or(AuthGuardError::AdminSessionRequired)?;

    touch_admin_session(db, &session.id, now)
        .await
        .map_err(|_| AuthGuardError::AdminSessionRequired)?;

    Ok(AdminSession {
        last_seen_at: now,
        ..session
    })
}

pub async fn require_app_device_session(
    db: &D1Database,
    req: &Request,
) -> std::result::Result<AppDeviceSession, AuthGuardError> {
    let token = app_device_token_from_request(req)
        .map_err(|_| AuthGuardError::AppSessionRequired)?
        .ok_or(AuthGuardError::AppSessionRequired)?;
    let token_hash = hash_session_token(&token);
    let now = now_timestamp();
    let session = get_valid_app_device_session_by_hash(db, &token_hash)
        .await
        .map_err(|_| AuthGuardError::AppSessionRequired)?
        .ok_or(AuthGuardError::AppSessionRequired)?;

    touch_app_device_session(db, &session.id, now)
        .await
        .map_err(|_| AuthGuardError::AppSessionRequired)?;

    Ok(AppDeviceSession {
        last_seen_at: Some(now),
        ..session
    })
}

pub async fn require_admin_request(
    db: &D1Database,
    req: &Request,
    env: &Env,
) -> std::result::Result<AdminSession, AuthGuardError> {
    let config = auth_config(env, req).map_err(|_| AuthGuardError::AuthConfigRequired(vec![]))?;

    if !config.configured {
        return Err(AuthGuardError::AuthConfigRequired(config.missing_config));
    }

    ensure_json_request(req)?;
    ensure_same_origin(req, &config)?;
    require_admin_session(db, req).await
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn local_rp_id(host: &str) -> String {
    if host == "::1" {
        "localhost".to_string()
    } else {
        host.to_string()
    }
}

pub async fn get_setting(db: &D1Database, key: &str) -> Result<Option<String>> {
    #[derive(serde::Deserialize)]
    struct SettingRow {
        value: String,
    }

    let args = [D1Type::Text(key)];
    Ok(db
        .prepare("SELECT value FROM settings WHERE key = ?")
        .bind_refs(&args)?
        .first::<SettingRow>(None)
        .await?
        .map(|row| row.value))
}

pub async fn get_setting_bool(db: &D1Database, key: &str) -> Result<bool> {
    Ok(matches!(
        get_setting(db, key).await?.as_deref(),
        Some("true") | Some("1")
    ))
}

pub async fn set_setting(db: &D1Database, key: &str, value: &str) -> Result<()> {
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

    Ok(())
}

pub async fn delete_setting(db: &D1Database, key: &str) -> Result<()> {
    let args = [D1Type::Text(key)];
    db.prepare("DELETE FROM settings WHERE key = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn mark_setup_completed(db: &D1Database, completed_at: i64) -> Result<()> {
    set_setting(db, SETUP_COMPLETED_KEY, "true").await?;
    set_setting(db, SETUP_COMPLETED_AT_KEY, &completed_at.to_string()).await
}

pub async fn is_setup_completed(db: &D1Database) -> Result<bool> {
    get_setting_bool(db, SETUP_COMPLETED_KEY).await
}

pub async fn can_use_setup_token(db: &D1Database) -> Result<bool> {
    Ok(!is_setup_completed(db).await? && count_admin_credentials(db).await? == 0)
}

pub async fn count_admin_credentials(db: &D1Database) -> Result<i64> {
    let count = db
        .prepare("SELECT COUNT(*) AS value FROM admin_credentials")
        .first::<CountValue>(None)
        .await?;

    Ok(count.map(|row| row.value).unwrap_or(0))
}

pub async fn list_admin_credentials(db: &D1Database) -> Result<Vec<AdminCredential>> {
    db.prepare(
        r#"
        SELECT credential_id, public_key, sign_count, name, created_at, last_used_at
        FROM admin_credentials
        ORDER BY created_at ASC
        "#,
    )
    .all()
    .await?
    .results()
}

pub async fn get_admin_credential(
    db: &D1Database,
    credential_id: &str,
) -> Result<Option<AdminCredential>> {
    let args = [D1Type::Text(credential_id)];
    db.prepare(
        r#"
        SELECT credential_id, public_key, sign_count, name, created_at, last_used_at
        FROM admin_credentials
        WHERE credential_id = ?
        "#,
    )
    .bind_refs(&args)?
    .first::<AdminCredential>(None)
    .await
}

pub async fn insert_admin_credential(
    db: &D1Database,
    credential: &NewAdminCredential,
) -> Result<()> {
    let args = [
        D1Type::Text(&credential.credential_id),
        D1Type::Text(&credential.public_key),
        D1Type::Integer(credential.sign_count as i32),
        D1Type::Text(&credential.name),
        D1Type::Integer(credential.created_at as i32),
    ];
    db.prepare(
        r#"
        INSERT INTO admin_credentials
            (credential_id, public_key, sign_count, name, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasskeyRegistrationFinalizeOperation {
    InsertCredential,
    MarkChallengeUsed,
    MarkSetupCompleted,
    MarkSetupCompletedAt,
}

const INITIAL_PASSKEY_FINALIZE_OPERATIONS: &[PasskeyRegistrationFinalizeOperation] = &[
    PasskeyRegistrationFinalizeOperation::InsertCredential,
    PasskeyRegistrationFinalizeOperation::MarkChallengeUsed,
    PasskeyRegistrationFinalizeOperation::MarkSetupCompleted,
    PasskeyRegistrationFinalizeOperation::MarkSetupCompletedAt,
];

const ADDITIONAL_PASSKEY_FINALIZE_OPERATIONS: &[PasskeyRegistrationFinalizeOperation] = &[
    PasskeyRegistrationFinalizeOperation::InsertCredential,
    PasskeyRegistrationFinalizeOperation::MarkChallengeUsed,
];

fn passkey_registration_finalize_operations(
    setup_allowed: bool,
) -> &'static [PasskeyRegistrationFinalizeOperation] {
    if setup_allowed {
        INITIAL_PASSKEY_FINALIZE_OPERATIONS
    } else {
        ADDITIONAL_PASSKEY_FINALIZE_OPERATIONS
    }
}

pub async fn finalize_passkey_registration(
    db: &D1Database,
    credential: &NewAdminCredential,
    challenge_id: &str,
    now: i64,
    setup_allowed: bool,
) -> Result<()> {
    let mut statements = Vec::new();
    let completed_at = now.to_string();

    for operation in passkey_registration_finalize_operations(setup_allowed) {
        let statement = match operation {
            PasskeyRegistrationFinalizeOperation::InsertCredential => {
                let args = [
                    D1Type::Text(&credential.credential_id),
                    D1Type::Text(&credential.public_key),
                    D1Type::Integer(credential.sign_count as i32),
                    D1Type::Text(&credential.name),
                    D1Type::Integer(credential.created_at as i32),
                ];
                db.prepare(
                    r#"
                    INSERT INTO admin_credentials
                        (credential_id, public_key, sign_count, name, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind_refs(&args)?
            }
            PasskeyRegistrationFinalizeOperation::MarkChallengeUsed => {
                let args = [D1Type::Integer(now as i32), D1Type::Text(challenge_id)];
                db.prepare("UPDATE auth_challenges SET used_at = ? WHERE id = ?")
                    .bind_refs(&args)?
            }
            PasskeyRegistrationFinalizeOperation::MarkSetupCompleted => {
                let args = [D1Type::Text(SETUP_COMPLETED_KEY), D1Type::Text("true")];
                db.prepare(
                    r#"
                    INSERT INTO settings (key, value)
                    VALUES (?, ?)
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value
                    "#,
                )
                .bind_refs(&args)?
            }
            PasskeyRegistrationFinalizeOperation::MarkSetupCompletedAt => {
                let args = [
                    D1Type::Text(SETUP_COMPLETED_AT_KEY),
                    D1Type::Text(&completed_at),
                ];
                db.prepare(
                    r#"
                    INSERT INTO settings (key, value)
                    VALUES (?, ?)
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value
                    "#,
                )
                .bind_refs(&args)?
            }
        };

        statements.push(statement);
    }

    db.batch(statements).await?;
    Ok(())
}

pub async fn update_admin_credential_usage(
    db: &D1Database,
    credential_id: &str,
    sign_count: i64,
    last_used_at: i64,
) -> Result<()> {
    let args = [
        D1Type::Integer(sign_count as i32),
        D1Type::Integer(last_used_at as i32),
        D1Type::Text(credential_id),
    ];
    db.prepare(
        r#"
        UPDATE admin_credentials
        SET sign_count = ?, last_used_at = ?
        WHERE credential_id = ?
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn delete_admin_credential(db: &D1Database, credential_id: &str) -> Result<()> {
    let args = [D1Type::Text(credential_id)];
    db.prepare("DELETE FROM admin_credentials WHERE credential_id = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn insert_auth_challenge(db: &D1Database, challenge: &NewAuthChallenge) -> Result<()> {
    let args = [
        D1Type::Text(&challenge.id),
        D1Type::Text(&challenge.challenge),
        D1Type::Text(&challenge.purpose),
        D1Type::Integer(challenge.created_at as i32),
        D1Type::Integer(challenge.expires_at as i32),
    ];
    db.prepare(
        r#"
        INSERT INTO auth_challenges
            (id, challenge, purpose, created_at, expires_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn get_valid_auth_challenge(
    db: &D1Database,
    id: &str,
    purpose: &str,
    now: i64,
) -> Result<Option<AuthChallenge>> {
    let args = [
        D1Type::Text(id),
        D1Type::Text(purpose),
        D1Type::Integer(now as i32),
    ];
    db.prepare(
        r#"
        SELECT id, challenge, purpose, created_at, expires_at, used_at
        FROM auth_challenges
        WHERE id = ? AND purpose = ? AND used_at IS NULL AND expires_at > ?
        "#,
    )
    .bind_refs(&args)?
    .first::<AuthChallenge>(None)
    .await
}

pub async fn mark_auth_challenge_used(db: &D1Database, id: &str, used_at: i64) -> Result<()> {
    let args = [D1Type::Integer(used_at as i32), D1Type::Text(id)];
    db.prepare("UPDATE auth_challenges SET used_at = ? WHERE id = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn insert_admin_session(db: &D1Database, session: &NewAdminSession) -> Result<()> {
    let credential_id = session.credential_id.as_deref();
    let args = [
        D1Type::Text(&session.id),
        D1Type::Text(&session.token_hash),
        D1Type::Text(credential_id.unwrap_or("")),
        D1Type::Integer(session.created_at as i32),
        D1Type::Integer(session.last_seen_at as i32),
        D1Type::Integer(session.expires_at as i32),
    ];
    db.prepare(
        r#"
        INSERT INTO admin_sessions
            (id, token_hash, credential_id, created_at, last_seen_at, expires_at)
        VALUES (?, ?, NULLIF(?, ''), ?, ?, ?)
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn get_valid_admin_session_by_hash(
    db: &D1Database,
    token_hash: &str,
    now: i64,
) -> Result<Option<AdminSession>> {
    let args = [D1Type::Text(token_hash), D1Type::Integer(now as i32)];
    db.prepare(
        r#"
        SELECT id, token_hash, credential_id, created_at, last_seen_at, expires_at, revoked_at
        FROM admin_sessions
        WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?
        "#,
    )
    .bind_refs(&args)?
    .first::<AdminSession>(None)
    .await
}

pub async fn list_admin_sessions(db: &D1Database) -> Result<Vec<AdminSession>> {
    db.prepare(
        r#"
        SELECT id, token_hash, credential_id, created_at, last_seen_at, expires_at, revoked_at
        FROM admin_sessions
        ORDER BY created_at DESC
        "#,
    )
    .all()
    .await?
    .results()
}

pub async fn touch_admin_session(
    db: &D1Database,
    session_id: &str,
    last_seen_at: i64,
) -> Result<()> {
    let args = [
        D1Type::Integer(last_seen_at as i32),
        D1Type::Text(session_id),
    ];
    db.prepare("UPDATE admin_sessions SET last_seen_at = ? WHERE id = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn revoke_admin_session(
    db: &D1Database,
    session_id: &str,
    revoked_at: i64,
) -> Result<()> {
    let args = [D1Type::Integer(revoked_at as i32), D1Type::Text(session_id)];
    db.prepare("UPDATE admin_sessions SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn revoke_all_admin_sessions(db: &D1Database, revoked_at: i64) -> Result<()> {
    let args = [D1Type::Integer(revoked_at as i32)];
    db.prepare("UPDATE admin_sessions SET revoked_at = ? WHERE revoked_at IS NULL")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn revoke_sessions_for_credential(
    db: &D1Database,
    credential_id: &str,
    revoked_at: i64,
) -> Result<()> {
    let args = [
        D1Type::Integer(revoked_at as i32),
        D1Type::Text(credential_id),
    ];
    db.prepare(
        r#"
        UPDATE admin_sessions
        SET revoked_at = ?
        WHERE credential_id = ? AND revoked_at IS NULL
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn insert_app_device_session(
    db: &D1Database,
    session: &NewAppDeviceSession,
) -> Result<()> {
    let issued_by_credential_id = session.issued_by_credential_id.as_deref();
    let args = [
        D1Type::Text(&session.id),
        D1Type::Text(&session.token_hash),
        D1Type::Text(&session.token_prefix),
        D1Type::Text(&session.name),
        D1Type::Text(issued_by_credential_id.unwrap_or("")),
        D1Type::Integer(session.created_at as i32),
    ];
    db.prepare(
        r#"
        INSERT INTO app_device_sessions
            (id, token_hash, token_prefix, name, issued_by_credential_id, created_at)
        VALUES (?, ?, ?, ?, NULLIF(?, ''), ?)
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn get_valid_app_device_session_by_hash(
    db: &D1Database,
    token_hash: &str,
) -> Result<Option<AppDeviceSession>> {
    let args = [D1Type::Text(token_hash)];
    db.prepare(
        r#"
        SELECT
            id, token_hash, token_prefix, name, issued_by_credential_id,
            created_at, last_seen_at, revoked_at
        FROM app_device_sessions
        WHERE token_hash = ? AND revoked_at IS NULL
        "#,
    )
    .bind_refs(&args)?
    .first::<AppDeviceSession>(None)
    .await
}

pub async fn list_app_device_sessions(db: &D1Database) -> Result<Vec<AppDeviceSession>> {
    db.prepare(
        r#"
        SELECT
            id, token_hash, token_prefix, name, issued_by_credential_id,
            created_at, last_seen_at, revoked_at
        FROM app_device_sessions
        ORDER BY created_at DESC
        "#,
    )
    .all()
    .await?
    .results()
}

pub async fn touch_app_device_session(
    db: &D1Database,
    session_id: &str,
    last_seen_at: i64,
) -> Result<()> {
    let args = [
        D1Type::Integer(last_seen_at as i32),
        D1Type::Text(session_id),
    ];
    db.prepare("UPDATE app_device_sessions SET last_seen_at = ? WHERE id = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn revoke_app_device_session(
    db: &D1Database,
    session_id: &str,
    revoked_at: i64,
) -> Result<()> {
    let args = [D1Type::Integer(revoked_at as i32), D1Type::Text(session_id)];
    db.prepare(
        "UPDATE app_device_sessions SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn revoke_all_app_device_sessions(db: &D1Database, revoked_at: i64) -> Result<()> {
    let args = [D1Type::Integer(revoked_at as i32)];
    db.prepare("UPDATE app_device_sessions SET revoked_at = ? WHERE revoked_at IS NULL")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn revoke_app_device_sessions_for_credential(
    db: &D1Database,
    credential_id: &str,
    revoked_at: i64,
) -> Result<()> {
    let args = [
        D1Type::Integer(revoked_at as i32),
        D1Type::Text(credential_id),
    ];
    db.prepare(
        r#"
        UPDATE app_device_sessions
        SET revoked_at = ?
        WHERE issued_by_credential_id = ? AND revoked_at IS NULL
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn get_auth_rate_limit(db: &D1Database, bucket: &str) -> Result<Option<AuthRateLimit>> {
    let args = [D1Type::Text(bucket)];
    db.prepare(
        r#"
        SELECT bucket, failed_count, first_failed_at, last_failed_at, locked_until
        FROM auth_rate_limits
        WHERE bucket = ?
        "#,
    )
    .bind_refs(&args)?
    .first::<AuthRateLimit>(None)
    .await
}

pub async fn record_auth_failure(
    db: &D1Database,
    bucket: &str,
    now: i64,
    locked_until: Option<i64>,
) -> Result<()> {
    let locked_until_value = locked_until.unwrap_or(0);
    let args = [
        D1Type::Text(bucket),
        D1Type::Integer(now as i32),
        D1Type::Integer(now as i32),
        D1Type::Integer(locked_until_value as i32),
    ];
    db.prepare(
        r#"
        INSERT INTO auth_rate_limits
            (bucket, failed_count, first_failed_at, last_failed_at, locked_until)
        VALUES (?, 1, ?, ?, NULLIF(?, 0))
        ON CONFLICT(bucket) DO UPDATE SET
            failed_count = failed_count + 1,
            last_failed_at = excluded.last_failed_at,
            locked_until = excluded.locked_until
        "#,
    )
    .bind_refs(&args)?
    .run()
    .await?;

    Ok(())
}

pub async fn clear_auth_rate_limit(db: &D1Database, bucket: &str) -> Result<()> {
    let args = [D1Type::Text(bucket)];
    db.prepare("DELETE FROM auth_rate_limits WHERE bucket = ?")
        .bind_refs(&args)?
        .run()
        .await?;

    Ok(())
}

pub async fn is_auth_rate_limited(db: &D1Database, bucket: &str, now: i64) -> Result<bool> {
    let Some(limit) = get_auth_rate_limit(db, bucket).await? else {
        return Ok(false);
    };

    if limit.locked_until.is_some_and(|locked_until| locked_until > now) {
        return Ok(true);
    }

    Ok(limit.failed_count >= AUTH_RATE_LIMIT_MAX_FAILURES
        && now - limit.first_failed_at <= AUTH_RATE_LIMIT_WINDOW_SECONDS)
}

pub async fn record_setup_auth_failure(db: &D1Database, bucket: &str, now: i64) -> Result<()> {
    let existing = get_auth_rate_limit(db, bucket).await?;
    let within_window = existing
        .as_ref()
        .is_some_and(|limit| now - limit.first_failed_at <= AUTH_RATE_LIMIT_WINDOW_SECONDS);
    let next_count = if within_window {
        existing
            .as_ref()
            .map(|limit| limit.failed_count + 1)
            .unwrap_or(1)
    } else {
        1
    };
    let locked_until =
        (next_count >= AUTH_RATE_LIMIT_MAX_FAILURES).then_some(now + AUTH_RATE_LIMIT_LOCK_SECONDS);

    if !within_window {
        clear_auth_rate_limit(db, bucket).await?;
    }

    record_auth_failure(db, bucket, now, locked_until).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_passkey_registration_includes_setup_writes() {
        assert_eq!(
            passkey_registration_finalize_operations(true),
            &[
                PasskeyRegistrationFinalizeOperation::InsertCredential,
                PasskeyRegistrationFinalizeOperation::MarkChallengeUsed,
                PasskeyRegistrationFinalizeOperation::MarkSetupCompleted,
                PasskeyRegistrationFinalizeOperation::MarkSetupCompletedAt,
            ]
        );
    }

    #[test]
    fn additional_passkey_registration_skips_setup_writes() {
        assert_eq!(
            passkey_registration_finalize_operations(false),
            &[
                PasskeyRegistrationFinalizeOperation::InsertCredential,
                PasskeyRegistrationFinalizeOperation::MarkChallengeUsed,
            ]
        );
    }

    #[test]
    fn app_device_token_prefix_keeps_public_hint_short() {
        assert_eq!(
            app_device_token_prefix("lwapp_abcdefghijklmnopqrstuvwxyz"),
            "lwapp_abcdefgh"
        );
    }
}
