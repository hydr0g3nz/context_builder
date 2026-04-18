use anyhow::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 2;

const DDL_V1: &str = r#"
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

const DDL_V2: &str = r#"
CREATE TABLE IF NOT EXISTS edges (
    src  INTEGER NOT NULL,
    dst  INTEGER NOT NULL,
    kind TEXT NOT NULL,
    meta TEXT,
    PRIMARY KEY (src, dst, kind)
);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src, kind);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst, kind);

CREATE TABLE IF NOT EXISTS edge_resolution (
    symbol_id   INTEGER NOT NULL,
    edge_kind   TEXT NOT NULL,
    resolved_at INTEGER NOT NULL,
    gopls_version TEXT,
    PRIMARY KEY (symbol_id, edge_kind)
);
"#;

pub fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL_V1)?;

    let current: u32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if current < 2 {
        conn.execute_batch(DDL_V2)?;
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
