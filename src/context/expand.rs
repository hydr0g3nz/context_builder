/// Expand a seed symbol set by BFS depth-1 over cached CALLS + IMPLEMENTS edges.
use anyhow::Result;
use std::collections::HashMap;

use crate::model::Symbol;
use crate::store::edges::{get_edges_from, get_edges_to, EdgeKind};
use crate::store::symbols::find_symbol_by_id;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct ExpandedNode {
    pub symbol: Symbol,
    /// Hop distance from the nearest seed (0 = seed itself)
    pub distance: usize,
    /// How many seeds directly reference this node
    pub seed_references: usize,
}

/// Expand seeds by BFS over cached edges. Stops at depth 1 per seed.
/// Returns all nodes (seeds + neighbors), deduplicated by symbol id.
pub fn expand(
    conn: &Connection,
    seeds: &[Symbol],
    cap: usize,
) -> Result<Vec<ExpandedNode>> {
    let mut nodes: HashMap<i64, ExpandedNode> = HashMap::new();

    // Insert seeds at distance 0
    for seed in seeds {
        if let Some(id) = seed.id {
            nodes.entry(id).or_insert(ExpandedNode {
                symbol: seed.clone(),
                distance: 0,
                seed_references: 0,
            });
        }
    }

    // BFS depth 1 from each seed
    for seed in seeds {
        let seed_id = match seed.id {
            Some(id) => id,
            None => continue,
        };

        let edge_kinds = [EdgeKind::Calls, EdgeKind::Implements];

        for kind in &edge_kinds {
            // Outgoing neighbors
            match get_edges_from(conn, seed_id, kind) {
                Ok(edges) => {
                    for edge in edges {
                        let neighbor_id = edge.dst;
                        if nodes.len() >= cap {
                            break;
                        }
                        let entry = nodes.entry(neighbor_id).or_insert_with(|| {
                            ExpandedNode {
                                symbol: Symbol {
                                    id: Some(neighbor_id),
                                    kind: crate::model::SymbolKind::Func,
                                    name: String::new(),
                                    package: String::new(),
                                    file: String::new(),
                                    line: 0,
                                    col: 0,
                                    line_end: None,
                                    signature: None,
                                    doc: None,
                                    visibility: crate::model::Visibility::Private,
                                    hash: None,
                                },
                                distance: 1,
                                seed_references: 0,
                            }
                        });
                        entry.seed_references += 1;
                    }
                }
                Err(e) => tracing::debug!("expand: edge lookup failed: {}", e),
            }

            // Incoming neighbors
            match get_edges_to(conn, seed_id, kind) {
                Ok(edges) => {
                    for edge in edges {
                        let neighbor_id = edge.src;
                        if nodes.len() >= cap {
                            break;
                        }
                        let entry = nodes.entry(neighbor_id).or_insert_with(|| {
                            ExpandedNode {
                                symbol: Symbol {
                                    id: Some(neighbor_id),
                                    kind: crate::model::SymbolKind::Func,
                                    name: String::new(),
                                    package: String::new(),
                                    file: String::new(),
                                    line: 0,
                                    col: 0,
                                    line_end: None,
                                    signature: None,
                                    doc: None,
                                    visibility: crate::model::Visibility::Private,
                                    hash: None,
                                },
                                distance: 1,
                                seed_references: 0,
                            }
                        });
                        entry.seed_references += 1;
                    }
                }
                Err(e) => tracing::debug!("expand: edge lookup failed: {}", e),
            }
        }
    }

    // Resolve stubs (distance=1 nodes that only have an id, no name yet)
    let mut result: Vec<ExpandedNode> = Vec::with_capacity(nodes.len());
    for (_id, node) in nodes {
        if node.symbol.name.is_empty() {
            // Resolve from DB
            match find_symbol_by_id(conn, node.symbol.id.unwrap_or(0)) {
                Ok(Some(sym)) => result.push(ExpandedNode {
                    symbol: sym,
                    distance: node.distance,
                    seed_references: node.seed_references,
                }),
                Ok(None) => {} // symbol no longer in index, skip
                Err(e) => tracing::debug!("expand: resolve stub failed: {}", e),
            }
        } else {
            result.push(node);
        }
    }

    Ok(result)
}
