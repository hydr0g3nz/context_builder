/// Shared helper: resolve a CLI symbol argument to a Symbol from the index.
use anyhow::{bail, Result};
use std::path::Path;
use std::time::Instant;

use crate::model::Symbol;
use crate::store::symbols::{find_symbols, FindQuery};
use crate::store::Store;

/// Open the index and resolve `sym_name` (e.g. "Save", "UserService.Save") to a Symbol.
/// `prefer_kind` biases the result toward a specific kind (e.g. "interface", "struct").
pub fn resolve_symbol(root: &Path, sym_name: &str) -> Result<(Store, Symbol)> {
    resolve_symbol_kind(root, sym_name, None)
}

pub fn resolve_symbol_kind(root: &Path, sym_name: &str, prefer_kind: Option<&str>) -> Result<(Store, Symbol)> {
    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        bail!("No gocx index found. Run `gocx init && gocx index` first.");
    }
    let store = Store::open(&db_path)?;

    let t = Instant::now();
    let q = FindQuery {
        query: sym_name,
        exact: false,
        kind: prefer_kind,
        package: None,
        limit: 10,
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
    // prefer method whose short name matches exactly: "Upload" -> "S3Uploader.Upload"
    let method_suffix = format!(".{}", sym_name);
    if let Some(method) = results.iter().find(|s| s.name.ends_with(&method_suffix)) {
        tracing::debug!(
            "resolve {:?} -> {}.{} via method suffix ({}ms)",
            sym_name,
            method.package,
            method.name,
            t.elapsed().as_millis()
        );
        return Ok((store, method.clone()));
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
