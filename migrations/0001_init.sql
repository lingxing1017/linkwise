CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    folder TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmarks_url
ON bookmarks (url);

CREATE INDEX IF NOT EXISTS idx_bookmarks_folder_order
ON bookmarks (folder, sort_order);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS folder_orders (
    parent_folder TEXT NOT NULL DEFAULT '',
    folder_name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (parent_folder, folder_name)
);

CREATE TABLE IF NOT EXISTS admin_credentials (
    credential_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    last_used_at INTEGER
);

CREATE TABLE IF NOT EXISTS auth_challenges (
    id TEXT PRIMARY KEY,
    challenge TEXT NOT NULL,
    purpose TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_auth_challenges_purpose_expires
ON auth_challenges (purpose, expires_at);

CREATE TABLE IF NOT EXISTS admin_sessions (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    credential_id TEXT,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_admin_sessions_credential_id
ON admin_sessions (credential_id);

CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires_revoked
ON admin_sessions (expires_at, revoked_at);

CREATE TABLE IF NOT EXISTS auth_rate_limits (
    bucket TEXT PRIMARY KEY,
    failed_count INTEGER NOT NULL DEFAULT 0,
    first_failed_at INTEGER NOT NULL,
    last_failed_at INTEGER NOT NULL,
    locked_until INTEGER
);
