use anyhow::Result;
use rusqlite::{Connection, params};

use crate::model::{Symbol, SymbolKind, Visibility};

pub fn insert_symbol(conn: &Connection, sym: &Symbol) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO symbols (kind, name, package, file, line, col, signature, doc, visibility, hash)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        params![
            sym.kind.as_str(),
            sym.name,
            sym.package,
            sym.file,
            sym.line,
            sym.col,
            sym.signature,
            sym.doc,
            sym.visibility.as_str(),
            sym.hash,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_symbols_batch(conn: &mut Connection, symbols: &[Symbol]) -> Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            r#"INSERT INTO symbols (kind, name, package, file, line, col, signature, doc, visibility, hash)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        )?;
        for sym in symbols {
            stmt.execute(params![
                sym.kind.as_str(),
                sym.name,
                sym.package,
                sym.file,
                sym.line,
                sym.col,
                sym.signature,
                sym.doc,
                sym.visibility.as_str(),
                sym.hash,
            ])?;
        }
    }
    tx.commit()?;
    Ok(symbols.len())
}

pub struct FindQuery<'a> {
    pub query: &'a str,
    pub exact: bool,
    pub kind: Option<&'a str>,
    pub package: Option<&'a str>,
    pub limit: usize,
}

pub fn find_symbols(conn: &Connection, q: &FindQuery) -> Result<Vec<Symbol>> {
    let mut conditions = vec![];
    let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

    if q.exact {
        conditions.push("name = ?".to_string());
        param_values.push(Box::new(q.query.to_string()));
    } else {
        conditions.push("name LIKE ? ESCAPE '\\'".to_string());
        let pattern = format!("%{}%", q.query.replace('%', "\\%").replace('_', "\\_"));
        param_values.push(Box::new(pattern));
    }

    if let Some(kind) = q.kind {
        conditions.push(format!("kind = ?"));
        param_values.push(Box::new(kind.to_string()));
    }

    if let Some(pkg) = q.package {
        conditions.push(format!("package = ?"));
        param_values.push(Box::new(pkg.to_string()));
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        r#"SELECT id, kind, name, package, file, line, col, signature, doc, visibility, hash
           FROM symbols
           WHERE {where_clause}
           ORDER BY
             CASE WHEN name = ? THEN 0
                  WHEN name LIKE ? THEN 1
                  ELSE 2 END,
             length(name)
           LIMIT ?"#
    );

    param_values.push(Box::new(q.query.to_string()));
    param_values.push(Box::new(format!("{}%", q.query)));
    param_values.push(Box::new(q.limit as i64));

    let params_refs: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let symbols = stmt
        .query_map(params_refs.as_slice(), row_to_symbol)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(symbols)
}

fn row_to_symbol(row: &rusqlite::Row) -> rusqlite::Result<Symbol> {
    let kind_str: String = row.get(1)?;
    let vis_str: String = row.get(9).unwrap_or_else(|_| "private".to_string());
    Ok(Symbol {
        id: Some(row.get(0)?),
        kind: SymbolKind::from_str(&kind_str).unwrap_or(SymbolKind::Func),
        name: row.get(2)?,
        package: row.get(3)?,
        file: row.get(4)?,
        line: row.get::<_, i64>(5)? as u32,
        col: row.get::<_, i64>(6)? as u32,
        signature: row.get(7)?,
        doc: row.get(8)?,
        visibility: if vis_str == "exported" {
            Visibility::Exported
        } else {
            Visibility::Private
        },
        hash: row.get(10)?,
    })
}

pub fn count_symbols_by_kind(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt =
        conn.prepare("SELECT kind, COUNT(*) as cnt FROM symbols GROUP BY kind ORDER BY cnt DESC")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn truncate_symbols(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM symbols", [])?;
    conn.execute("DELETE FROM files", [])?;
    Ok(())
}

pub fn packages_with_symbols(
    conn: &Connection,
) -> Result<Vec<(String, Vec<(String, i64)>)>> {
    let mut pkg_stmt =
        conn.prepare("SELECT DISTINCT package FROM symbols ORDER BY package")?;
    let packages: Vec<String> = pkg_stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut result = vec![];
    for pkg in packages {
        let mut kind_stmt = conn.prepare(
            "SELECT kind, COUNT(*) FROM symbols WHERE package = ?1 GROUP BY kind ORDER BY kind",
        )?;
        let kinds: Vec<(String, i64)> = kind_stmt
            .query_map(params![pkg], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        result.push((pkg, kinds));
    }
    Ok(result)
}
