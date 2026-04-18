/// Interface implementation resolution via gopls + edge cache.
use anyhow::Result;

use crate::gopls::queries::uri_to_rel_path;
use crate::gopls::GoplsClient;
use crate::model::Symbol;
use crate::store::edges::{
    get_edges_from, get_edges_to, is_resolved, mark_resolved, upsert_edges_batch, Edge, EdgeKind,
};
use crate::store::symbols::find_symbols_at_location;
use crate::store::Store;

/// Find all structs/types that implement `iface`.
pub async fn find_implementations(
    store: &Store,
    client: &mut GoplsClient,
    iface: &Symbol,
) -> Result<Vec<Symbol>> {
    let iface_id = match iface.id {
        Some(id) => id,
        None => return Ok(vec![]),
    };

    if !is_resolved(&store.conn, iface_id, &EdgeKind::Implements)? {
        resolve_impls(store, client, iface).await?;
    }

    // Return all dst symbols in IMPLEMENTS edges where src = iface
    let edges = get_edges_from(&store.conn, iface_id, &EdgeKind::Implements)?;
    let mut result = Vec::new();
    for edge in edges {
        if let Some(sym) = crate::store::symbols::find_symbol_by_id(&store.conn, edge.dst)? {
            result.push(sym);
        }
    }
    Ok(result)
}

/// Find all interfaces that `concrete` implements.
pub async fn find_interfaces_for(
    store: &Store,
    client: &mut GoplsClient,
    concrete: &Symbol,
) -> Result<Vec<Symbol>> {
    // Reverse: find IMPLEMENTS edges where dst = concrete
    let concrete_id = match concrete.id {
        Some(id) => id,
        None => return Ok(vec![]),
    };

    // Attempt to resolve for this symbol too (gopls can return both directions)
    if !is_resolved(&store.conn, concrete_id, &EdgeKind::Implements)? {
        resolve_impls(store, client, concrete).await?;
    }

    let edges = get_edges_to(&store.conn, concrete_id, &EdgeKind::Implements)?;
    let mut result = Vec::new();
    for edge in edges {
        if let Some(sym) = crate::store::symbols::find_symbol_by_id(&store.conn, edge.src)? {
            result.push(sym);
        }
    }
    Ok(result)
}

async fn resolve_impls(store: &Store, client: &mut GoplsClient, sym: &Symbol) -> Result<()> {
    let sym_id = match sym.id {
        Some(id) => id,
        None => return Ok(()),
    };

    let gopls_version = client.server_version.clone();
    let mut new_edges: Vec<Edge> = Vec::new();

    let root_uri = client.root_uri.clone();

    match client.implementations(sym).await {
        Ok(locs) => {
            for loc in locs {
                let path = uri_to_rel_path(&loc.uri, &root_uri);
                let line = loc.range.start.line as usize + 1;
                let col = loc.range.start.character as usize;
                match find_symbols_at_location(&store.conn, &path, line, col) {
                    Ok(Some(impl_sym)) => {
                        if let Some(impl_id) = impl_sym.id {
                            new_edges.push(Edge {
                                src: sym_id,
                                dst: impl_id,
                                kind: EdgeKind::Implements,
                                meta: Some(serde_json::json!({"via": "gopls"})),
                            });
                        }
                    }
                    Ok(None) => tracing::debug!("no symbol in index at {}:{}:{}", path, line, col),
                    Err(e) => tracing::warn!("location lookup error for impl: {}", e),
                }
            }
        }
        Err(e) => tracing::warn!("implementations query failed for {}: {}", sym.name, e),
    }

    upsert_edges_batch(&store.conn, &new_edges)?;
    mark_resolved(
        &store.conn,
        sym_id,
        &EdgeKind::Implements,
        gopls_version.as_deref(),
    )?;
    Ok(())
}
