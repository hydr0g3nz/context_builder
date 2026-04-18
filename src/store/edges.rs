use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeKind {
    Calls,
    Implements,
    UsesType,
    Embeds,
    References,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Calls => "CALLS",
            EdgeKind::Implements => "IMPLEMENTS",
            EdgeKind::UsesType => "USES_TYPE",
            EdgeKind::Embeds => "EMBEDS",
            EdgeKind::References => "REFERENCES",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "CALLS" => Some(EdgeKind::Calls),
            "IMPLEMENTS" => Some(EdgeKind::Implements),
            "USES_TYPE" => Some(EdgeKind::UsesType),
            "EMBEDS" => Some(EdgeKind::Embeds),
            "REFERENCES" => Some(EdgeKind::References),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub src: i64,
    pub dst: i64,
    pub kind: EdgeKind,
    pub meta: Option<serde_json::Value>,
}

/// Insert or ignore an edge (idempotent due to PRIMARY KEY).
pub fn upsert_edge(conn: &Connection, edge: &Edge) -> Result<()> {
    let meta = edge.meta.as_ref().map(|m| m.to_string());
    conn.execute(
        "INSERT OR IGNORE INTO edges (src, dst, kind, meta) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![edge.src, edge.dst, edge.kind.as_str(), meta],
    )?;
    Ok(())
}

/// Batch upsert edges in a single transaction.
pub fn upsert_edges_batch(conn: &Connection, edges: &[Edge]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO edges (src, dst, kind, meta) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for e in edges {
            let meta = e.meta.as_ref().map(|m| m.to_string());
            stmt.execute(rusqlite::params![e.src, e.dst, e.kind.as_str(), meta])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Return all outgoing edges from `src` with a given kind.
pub fn get_edges_from(conn: &Connection, src: i64, kind: &EdgeKind) -> Result<Vec<Edge>> {
    let mut stmt = conn.prepare_cached(
        "SELECT src, dst, kind, meta FROM edges WHERE src = ?1 AND kind = ?2",
    )?;
    let edges = stmt
        .query_map(rusqlite::params![src, kind.as_str()], |row| {
            let meta_str: Option<String> = row.get(3)?;
            Ok((row.get(0)?, row.get(1)?, row.get::<_, String>(2)?, meta_str))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(src, dst, kind_str, meta_str)| {
            let kind = EdgeKind::parse(&kind_str)?;
            let meta = meta_str.and_then(|s| serde_json::from_str(&s).ok());
            Some(Edge { src, dst, kind, meta })
        })
        .collect();
    Ok(edges)
}

/// Return all incoming edges to `dst` with a given kind.
pub fn get_edges_to(conn: &Connection, dst: i64, kind: &EdgeKind) -> Result<Vec<Edge>> {
    let mut stmt = conn.prepare_cached(
        "SELECT src, dst, kind, meta FROM edges WHERE dst = ?1 AND kind = ?2",
    )?;
    let edges = stmt
        .query_map(rusqlite::params![dst, kind.as_str()], |row| {
            let meta_str: Option<String> = row.get(3)?;
            Ok((row.get(0)?, row.get(1)?, row.get::<_, String>(2)?, meta_str))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(src, dst, kind_str, meta_str)| {
            let kind = EdgeKind::parse(&kind_str)?;
            let meta = meta_str.and_then(|s| serde_json::from_str(&s).ok());
            Some(Edge { src, dst, kind, meta })
        })
        .collect();
    Ok(edges)
}

/// Check if a symbol's edges of a given kind have already been resolved.
pub fn is_resolved(conn: &Connection, symbol_id: i64, kind: &EdgeKind) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM edge_resolution WHERE symbol_id = ?1 AND edge_kind = ?2",
        rusqlite::params![symbol_id, kind.as_str()],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Mark a symbol's edges of a given kind as resolved.
pub fn mark_resolved(
    conn: &Connection,
    symbol_id: i64,
    kind: &EdgeKind,
    gopls_version: Option<&str>,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    conn.execute(
        "INSERT OR REPLACE INTO edge_resolution (symbol_id, edge_kind, resolved_at, gopls_version) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![symbol_id, kind.as_str(), now as i64, gopls_version],
    )?;
    Ok(())
}

/// Delete all edges where src or dst file matches `file_path` (for incremental invalidation).
pub fn invalidate_file_edges(conn: &Connection, file_path: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM edges WHERE src IN (SELECT id FROM symbols WHERE file = ?1) OR dst IN (SELECT id FROM symbols WHERE file = ?1)",
        rusqlite::params![file_path],
    )?;
    conn.execute(
        "DELETE FROM edge_resolution WHERE symbol_id IN (SELECT id FROM symbols WHERE file = ?1)",
        rusqlite::params![file_path],
    )?;
    Ok(())
}
