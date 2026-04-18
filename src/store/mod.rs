pub mod edges;
pub mod files;
pub mod schema;
pub mod symbols;

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct Store {
    pub conn: Connection,
}

impl Store {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        schema::apply_migrations(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_or_create(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::open(db_path)
    }
}
