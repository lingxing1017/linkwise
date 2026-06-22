#![allow(dead_code)]

use crate::crypto;
use crate::models::{AdminCredential, AdminSession, AuthChallenge, CountValue};
use js_sys::wasm_bindgen::JsCast;
use js_sys::{Reflect, Uint8Array};
use worker::d1::{D1Database, D1Type};
use worker::*;

pub const SETUP_COMPLETED_KEY: &str = "auth.setup_completed";
pub const SETUP_COMPLETED_AT_KEY: &str = "auth.setup_completed_at";
pub const PURPOSE_PASSKEY_REGISTRATION: &str = "passkey_registration";
pub const PURPOSE_PASSKEY_LOGIN: &str = "passkey_login";
pub const WEB_SESSION_MAX_AGE_SECONDS: i64 = 24 * 60 * 60;

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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthRateLimit {
    pub bucket: String,
    pub failed_count: i64,
    pub first_failed_at: i64,
    pub last_failed_at: i64,
    pub locked_until: Option<i64>,
}

pub fn now_timestamp() -> i64 {
    (js_sys::Date::now() / 1000.0).floor() as i64
}

pub fn hash_session_token(token: &str) -> String {
    crypto::sha256_hex(token)
}

pub fn random_base64url(byte_len: u32) -> Result<String> {
    let bytes = random_bytes(byte_len)?;
    Ok(base64url_encode(&bytes))
}

pub fn base64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
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

pub async fn touch_admin_session(db: &D1Database, session_id: &str, last_seen_at: i64) -> Result<()> {
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

pub async fn revoke_admin_session(db: &D1Database, session_id: &str, revoked_at: i64) -> Result<()> {
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
