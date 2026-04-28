/// Per-callsite interface resolution: given a symbol, return its implementations if it's an interface method.
use anyhow::Result;

use crate::gopls::GoplsClient;
use crate::model::{Symbol, SymbolKind};
use crate::semantic::impls::find_implementations;
use crate::store::Store;

/// If `sym` is an Interface (or a method on an interface), return all implementations.
/// Returns empty vec if not an interface type.
pub async fn resolve_interface_impls(
    store: &Store,
    client: &mut GoplsClient,
    sym: &Symbol,
) -> Result<Vec<Symbol>> {
    match sym.kind {
        SymbolKind::Interface => find_implementations(store, client, sym).await,
        SymbolKind::Method => {
            // Look up the receiver type; if it's an interface, find all impls
            let receiver = sym.name.split('.').next().unwrap_or(&sym.name);
            let q = crate::store::symbols::FindQuery {
                query: receiver,
                exact: true,
                kind: Some("interface"),
                package: Some(&sym.package),
                limit: 1,
            };
            match crate::store::symbols::find_symbols(&store.conn, &q)? {
                v if !v.is_empty() => find_implementations(store, client, &v[0]).await,
                _ => Ok(vec![]),
            }
        }
        _ => Ok(vec![]),
    }
}
