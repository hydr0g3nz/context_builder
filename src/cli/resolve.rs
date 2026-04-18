/// Shared helper: resolve a CLI symbol argument to a Symbol from the index.
use anyhow::{bail, Result};
use std::path::Path;
use std::time::Instant;

use crate::model::Symbol;
use crate::store::symbols::{find_symbols, FindQuery};
use crate::store::Store;

/// Open the index and resolve `sym_name` (e.g. "Save", "UserService.Save") to a Symbol.
pub fn resolve_symbol(root: &Path, sym_name: &str) -> Result<(Store, Symbol)> {
    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        bail!("No gocx index found. Run `gocx init && gocx index` first.");
    }
    let store = Store::open(&db_path)?;

    let t = Instant::now();
    let q = FindQuery {
        query: sym_name,
        exact: false,
        kind: None,
        package: None,
        limit: 5,
    };
    let mut results = find_symbols(&store.conn, &q)?;
    if results.is_empty() {
        bail!("Symbol {:?} not found in index.", sym_name);
    }
    // prefer exact match
    if let Some(exact) = results.iter().find(|s| s.name == sym_name) {
        tracing::debug!(
            "resolve {:?} -> {}.{} ({}ms)",
            sym_name,
            exact.package,
            exact.name,
            t.elapsed().as_millis()
        );
        return Ok((store, exact.clone()));
    }
    let sym = results.remove(0);
    tracing::debug!(
        "resolve {:?} -> {}.{} ({}ms)",
        sym_name,
        sym.package,
        sym.name,
        t.elapsed().as_millis()
    );
    Ok((store, sym))
}
