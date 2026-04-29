/// On-demand call graph: BFS from a seed symbol via gopls, cached in SQLite edges table.
use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::all_simple_paths;
use std::collections::{HashMap, VecDeque};

use crate::gopls::queries::uri_to_rel_path;
use crate::gopls::GoplsClient;
use crate::model::Symbol;
use crate::store::edges::{
    get_edges_from, get_edges_to, is_resolved, mark_resolved, upsert_edges_batch, Edge, EdgeKind,
};
use crate::store::symbols::{find_symbol_by_id, find_symbols_at_location};
use crate::store::Store;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CallNode {
    pub symbol: Symbol,
    pub depth: usize,
}

/// Resolve callers of `root` up to `max_depth` hops, returning a flat list.
pub async fn callers(
    store: &Store,
    client: &mut GoplsClient,
    root: &Symbol,
    max_depth: usize,
) -> Result<Vec<CallNode>> {
    bfs_edges(store, client, root, max_depth, Direction::Incoming).await
}

/// Resolve callees of `root` up to `max_depth` hops.
pub async fn callees(
    store: &Store,
    client: &mut GoplsClient,
    root: &Symbol,
    max_depth: usize,
) -> Result<Vec<CallNode>> {
    bfs_edges(store, client, root, max_depth, Direction::Outgoing).await
}

enum Direction {
    Incoming,
    Outgoing,
}

async fn bfs_edges(
    store: &Store,
    client: &mut GoplsClient,
    root: &Symbol,
    max_depth: usize,
    dir: Direction,
) -> Result<Vec<CallNode>> {
    let mut visited: HashMap<i64, usize> = HashMap::new();
    let mut queue: VecDeque<(Symbol, usize)> = VecDeque::new();
    let mut results: Vec<CallNode> = Vec::new();

    queue.push_back((root.clone(), 0));

    while let Some((sym, depth)) = queue.pop_front() {
        let sym_id = match sym.id {
            Some(id) => id,
            None => continue,
        };

        if visited.contains_key(&sym_id) {
            continue;
        }
        visited.insert(sym_id, depth);

        if depth > 0 {
            results.push(CallNode { symbol: sym.clone(), depth });
        }

        if depth >= max_depth {
            continue;
        }

        // Resolve edges from gopls if not cached
        if !is_resolved(&store.conn, sym_id, &EdgeKind::Calls)? {
            tracing::debug!("edge cache miss for {} — querying gopls", sym.name);
            resolve_and_cache(store, client, &sym).await?;
        } else {
            tracing::debug!("edge cache hit for {}", sym.name);
        }

        // BFS neighbors from cache
        let neighbors = match dir {
            Direction::Incoming => get_edges_to(&store.conn, sym_id, &EdgeKind::Calls)?,
            Direction::Outgoing => get_edges_from(&store.conn, sym_id, &EdgeKind::Calls)?,
        };

        for edge in neighbors {
            let neighbor_id = match dir {
                Direction::Incoming => edge.src,
                Direction::Outgoing => edge.dst,
            };
            if !visited.contains_key(&neighbor_id) {
                if let Some(neighbor_sym) = find_symbol_by_id(&store.conn, neighbor_id)? {
                    queue.push_back((neighbor_sym, depth + 1));
                }
            }
        }
    }

    Ok(results)
}

/// Ask gopls for callers + callees of `sym` and persist to edges table.
pub async fn resolve_and_cache_callees(store: &Store, client: &mut GoplsClient, sym: &Symbol) -> Result<()> {
    resolve_and_cache(store, client, sym).await
}

async fn resolve_and_cache(store: &Store, client: &mut GoplsClient, sym: &Symbol) -> Result<()> {
    let sym_id = match sym.id {
        Some(id) => id,
        None => return Ok(()),
    };

    let gopls_version = client.server_version.clone();
    let mut new_edges: Vec<Edge> = Vec::new();

    let root_uri = client.root_uri.clone();

    // Outgoing: callees
    match client.callees(sym).await {
        Ok(calls) => {
            for call in calls {
                let callee_path = uri_to_rel_path(&call.to.uri, &root_uri);
                let line = call.to.selection_range.start.line as usize + 1;
                let col = call.to.selection_range.start.character as usize + 1;
                match find_symbols_at_location(&store.conn, &callee_path, line, col) {
                    Ok(Some(callee)) => {
                        if let Some(callee_id) = callee.id {
                            new_edges.push(Edge {
                                src: sym_id,
                                dst: callee_id,
                                kind: EdgeKind::Calls,
                                meta: Some(serde_json::json!({
                                    "line": line,
                                    "col": col,
                                })),
                            });
                        }
                    }
                    Ok(None) => tracing::debug!("no symbol in index at {}:{}:{}", callee_path, line, col),
                    Err(e) => tracing::warn!("location lookup error for callee: {}", e),
                }
            }
        }
        Err(e) => tracing::warn!("callees query failed for {}: {}", sym.name, e),
    }

    // Incoming: callers
    match client.callers(sym).await {
        Ok(calls) => {
            for call in calls {
                let caller_path = uri_to_rel_path(&call.from.uri, &root_uri);
                let line = call.from.selection_range.start.line as usize + 1;
                let col = call.from.selection_range.start.character as usize + 1;
                match find_symbols_at_location(&store.conn, &caller_path, line, col) {
                    Ok(Some(caller)) => {
                        if let Some(caller_id) = caller.id {
                            new_edges.push(Edge {
                                src: caller_id,
                                dst: sym_id,
                                kind: EdgeKind::Calls,
                                meta: Some(serde_json::json!({
                                    "line": line,
                                    "col": col,
                                })),
                            });
                        }
                    }
                    Ok(None) => tracing::debug!("no symbol in index at {}:{}:{}", caller_path, line, col),
                    Err(e) => tracing::warn!("location lookup error for caller: {}", e),
                }
            }
        }
        Err(e) => tracing::warn!("callers query failed for {}: {}", sym.name, e),
    }

    tracing::debug!("caching {} edges for {}", new_edges.len(), sym.name);
    upsert_edges_batch(&store.conn, &new_edges)?;
    mark_resolved(
        &store.conn,
        sym_id,
        &EdgeKind::Calls,
        gopls_version.as_deref(),
    )?;

    Ok(())
}

/// Find shortest path between `from` and `to` using BFS over CALLS edges.
/// Returns the path as an ordered list of symbols, or empty if no path found.
pub async fn trace_path(
    store: &Store,
    client: &mut GoplsClient,
    from: &Symbol,
    to: &Symbol,
    max_depth: usize,
) -> Result<Vec<Symbol>> {
    // Collect a limited subgraph via BFS from `from`
    let reachable = callees(store, client, from, max_depth).await?;

    // Build petgraph
    let mut graph: DiGraph<i64, ()> = DiGraph::new();
    let mut id_to_node: HashMap<i64, NodeIndex> = HashMap::new();

    let ensure_node = |graph: &mut DiGraph<i64, ()>, id: i64, map: &mut HashMap<i64, NodeIndex>| -> NodeIndex {
        *map.entry(id).or_insert_with(|| graph.add_node(id))
    };

    let from_id = from.id.unwrap_or(0);
    let to_id = to.id.unwrap_or(0);

    ensure_node(&mut graph, from_id, &mut id_to_node);
    for node in &reachable {
        if let Some(nid) = node.symbol.id {
            ensure_node(&mut graph, nid, &mut id_to_node);
        }
    }

    // Add edges from cache — include the root node itself
    let all_ids: Vec<i64> = std::iter::once(from_id)
        .chain(reachable.iter().filter_map(|n| n.symbol.id))
        .collect();
    for nid in all_ids {
        if let Ok(edges) = get_edges_from(&store.conn, nid, &EdgeKind::Calls) {
            for edge in edges {
                if let (Some(&src_node), Some(&dst_node)) =
                    (id_to_node.get(&edge.src), id_to_node.get(&edge.dst))
                {
                    graph.add_edge(src_node, dst_node, ());
                }
            }
        }
    }

    let from_node = match id_to_node.get(&from_id) {
        Some(n) => *n,
        None => return Ok(vec![]),
    };
    let to_node = match id_to_node.get(&to_id) {
        Some(n) => *n,
        None => return Ok(vec![]),
    };

    // Find a path
    let paths: Vec<Vec<NodeIndex>> = all_simple_paths(&graph, from_node, to_node, 0, Some(max_depth))
        .take(1)
        .collect();

    if paths.is_empty() {
        return Ok(vec![]);
    }

    // Resolve symbols for each node in the path
    let mut result = Vec::new();
    for node_idx in &paths[0] {
        let sym_id = graph[*node_idx];
        if let Some(sym) = find_symbol_by_id(&store.conn, sym_id)? {
            result.push(sym);
        }
    }

    Ok(result)
}
