use anyhow::Result;
use rusqlite::{params, Connection};

use crate::model::FileRecord;

pub fn upsert_file(conn: &Connection, rec: &FileRecord) -> Result<()> {
    conn.execute(
        r#"INSERT OR REPLACE INTO files (path, hash, mtime, parsed_at, package)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
        params![rec.path, rec.hash, rec.mtime, rec.parsed_at, rec.package],
    )?;
    Ok(())
}

pub fn get_file(conn: &Connection, path: &str) -> Result<Option<FileRecord>> {
    let result = conn.query_row(
        "SELECT path, hash, mtime, parsed_at, package FROM files WHERE path = ?1",
        params![path],
        |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                hash: row.get(1)?,
                mtime: row.get(2)?,
                parsed_at: row.get(3)?,
                package: row.get(4)?,
            })
        },
    );
    match result {
        Ok(rec) => Ok(Some(rec)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn count_files(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?)
}

pub fn delete_file_symbols(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM symbols WHERE file = ?1", params![path])?;
    conn.execute("DELETE FROM files WHERE path = ?1", params![path])?;
    Ok(())
}
