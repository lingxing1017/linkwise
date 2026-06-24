CREATE TABLE IF NOT EXISTS app_device_sessions (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    issued_by_credential_id TEXT,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER,
    revoked_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_app_device_sessions_issued_by_credential_id
ON app_device_sessions (issued_by_credential_id);

CREATE INDEX IF NOT EXISTS idx_app_device_sessions_revoked
ON app_device_sessions (revoked_at);
