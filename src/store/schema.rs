use anyhow::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 1;

const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE IF NOT EXISTS files (
    path      TEXT PRIMARY KEY,
    hash      TEXT NOT NULL,
    mtime     INTEGER NOT NULL,
    parsed_at INTEGER NOT NULL,
    package   TEXT
);

CREATE TABLE IF NOT EXISTS symbols (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL,
    package    TEXT NOT NULL,
    file       TEXT NOT NULL,
    line       INTEGER NOT NULL,
    col        INTEGER NOT NULL,
    signature  TEXT,
    doc        TEXT,
    visibility TEXT,
    hash       TEXT
);

CREATE INDEX IF NOT EXISTS idx_symbols_name         ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_name_nocase  ON symbols(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_symbols_package      ON symbols(package);
CREATE INDEX IF NOT EXISTS idx_symbols_file         ON symbols(file);
CREATE INDEX IF NOT EXISTS idx_symbols_kind         ON symbols(kind);
"#;

pub fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    let current: Option<u32> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok());

    if current.is_none() {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            rusqlite::params![SCHEMA_VERSION.to_string()],
        )?;
    }
    Ok(())
}

pub fn get_schema_version(conn: &Connection) -> Option<u32> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
}
