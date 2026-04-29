/// Build a hierarchical call+control-flow tree from a root symbol.
use anyhow::Result;
use std::collections::HashSet;

use crate::flow::controlflow::ControlFlowExtractor;
use crate::flow::interface::resolve_interface_impls;
use crate::gopls::GoplsClient;
use crate::model::{Symbol, SymbolKind};
use crate::semantic::call_graph::resolve_and_cache_callees;
use crate::store::edges::{get_edges_from, EdgeKind};
use crate::store::symbols::find_symbol_by_id;
use crate::store::Store;

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowNodeKind {
    Root,
    Call,
    Intf,
    Impl,
    If,
    Else,
    Switch,
    TypeSwitch,
    Select,
    Case,
    Go,
    Defer,
}

impl FlowNodeKind {
    pub fn tag(&self) -> &'static str {
        match self {
            FlowNodeKind::Root => "ROOT",
            FlowNodeKind::Call => "CALL",
            FlowNodeKind::Intf => "INTF",
            FlowNodeKind::Impl => "IMPL",
            FlowNodeKind::If => "IF",
            FlowNodeKind::Else => "ELSE",
            FlowNodeKind::Switch => "SWITCH",
            FlowNodeKind::TypeSwitch => "TYPESWITCH",
            FlowNodeKind::Select => "SELECT",
            FlowNodeKind::Case => "CASE",
            FlowNodeKind::Go => "GO",
            FlowNodeKind::Defer => "DEFER",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FlowNode {
    pub kind: FlowNodeKind,
    pub label: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    /// L3: function/method signature
    pub signature: Option<String>,
    pub symbol_id: Option<i64>,
    pub children: Vec<FlowNode>,
    pub truncated_reason: Option<String>,
}

pub struct FlowOptions {
    pub max_depth: usize,
    pub exclude_patterns: Vec<String>,
}

pub async fn build_flow(
    store: &Store,
    client: &mut GoplsClient,
    root: &Symbol,
    opts: &FlowOptions,
) -> Result<FlowNode> {
    let mut visited: HashSet<i64> = HashSet::new();
    let mut cf_extractor = ControlFlowExtractor::new()?;

    let root_node = FlowNode {
        kind: FlowNodeKind::Root,
        label: root.name.clone(),
        file: root.file.clone(),
        line: root.line,
        col: root.col,
        signature: root.signature.clone(),
        symbol_id: root.id,
        children: vec![],
        truncated_reason: None,
    };

    Box::pin(expand_node(
        root_node,
        root.clone(),
        store,
        client,
        &mut cf_extractor,
        opts,
        0,
        &mut visited,
    ))
    .await
}

#[allow(clippy::too_many_arguments)]
async fn expand_node(
    mut node: FlowNode,
    sym: Symbol,
    store: &Store,
    client: &mut GoplsClient,
    cf_extractor: &mut ControlFlowExtractor,
    opts: &FlowOptions,
    depth: usize,
    visited: &mut HashSet<i64>,
) -> Result<FlowNode> {
    let sym_id = match sym.id {
        Some(id) => id,
        None => return Ok(node),
    };

    if visited.contains(&sym_id) {
        node.truncated_reason = Some("cycle".to_string());
        return Ok(node);
    }
    visited.insert(sym_id);

    if depth >= opts.max_depth {
        node.truncated_reason = Some("max_depth".to_string());
        visited.remove(&sym_id);
        return Ok(node);
    }

    // Ensure call edges are cached
    resolve_and_cache_callees(store, client, &sym).await?;

    // Get callee edges (outgoing CALLS)
    let callee_edges = get_edges_from(&store.conn, sym_id, &EdgeKind::Calls)?;

    // Build call child nodes
    let mut call_items: Vec<(u32, FlowNode)> = Vec::new();
    for edge in callee_edges {
        let call_line = edge
            .meta
            .as_ref()
            .and_then(|m| m.get("line"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0);

        let call_col = edge
            .meta
            .as_ref()
            .and_then(|m| m.get("col"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        let callee_sym = match find_symbol_by_id(&store.conn, edge.dst)? {
            Some(s) => s,
            None => continue,
        };

        if opts
            .exclude_patterns
            .iter()
            .any(|pat| callee_sym.file.contains(pat.as_str()))
        {
            continue;
        }

        let is_intf = matches!(callee_sym.kind, SymbolKind::Interface);

        let call_node = FlowNode {
            kind: if is_intf {
                FlowNodeKind::Intf
            } else {
                FlowNodeKind::Call
            },
            label: callee_sym.name.clone(),
            file: callee_sym.file.clone(),
            line: call_line,
            col: call_col,
            signature: callee_sym.signature.clone(),
            symbol_id: callee_sym.id,
            children: vec![],
            truncated_reason: None,
        };

        let expanded = if is_intf {
            let mut intf_node = call_node;
            let impls = resolve_interface_impls(store, client, &callee_sym).await?;
            for impl_sym in impls {
                if opts
                    .exclude_patterns
                    .iter()
                    .any(|pat| impl_sym.file.contains(pat.as_str()))
                {
                    continue;
                }
                let impl_node = FlowNode {
                    kind: FlowNodeKind::Impl,
                    label: impl_sym.name.clone(),
                    file: impl_sym.file.clone(),
                    line: impl_sym.line,
                    col: impl_sym.col,
                    signature: impl_sym.signature.clone(),
                    symbol_id: impl_sym.id,
                    children: vec![],
                    truncated_reason: None,
                };
                let expanded_impl = Box::pin(expand_node(
                    impl_node,
                    impl_sym,
                    store,
                    client,
                    cf_extractor,
                    opts,
                    depth + 1,
                    visited,
                ))
                .await?;
                intf_node.children.push(expanded_impl);
            }
            intf_node
        } else {
            Box::pin(expand_node(
                call_node,
                callee_sym,
                store,
                client,
                cf_extractor,
                opts,
                depth + 1,
                visited,
            ))
            .await?
        };

        call_items.push((call_line, expanded));
    }

    // Extract control-flow nodes within this function's body
    let body_start = sym.line;
    let body_end = sym.line_end.unwrap_or(sym.line + 200);

    let source = std::fs::read_to_string(&sym.file)
        .or_else(|_| {
            let cwd = std::env::current_dir().unwrap_or_default();
            std::fs::read_to_string(cwd.join(&sym.file))
        })
        .ok();

    let mut cf_items: Vec<(u32, FlowNode)> = Vec::new();
    if let Some(ref src) = source {
        match cf_extractor.extract_in_range(src, body_start, body_end) {
            Ok(cf_nodes) => {
                use crate::flow::controlflow::CfKind;
                for cf in cf_nodes {
                    let kind = match cf.kind {
                        CfKind::If => FlowNodeKind::If,
                        CfKind::Else => FlowNodeKind::Else,
                        CfKind::Switch => FlowNodeKind::Switch,
                        CfKind::TypeSwitch => FlowNodeKind::TypeSwitch,
                        CfKind::Select => FlowNodeKind::Select,
                        CfKind::CommCase => FlowNodeKind::Case,
                        CfKind::Go => FlowNodeKind::Go,
                        CfKind::Defer => FlowNodeKind::Defer,
                    };
                    cf_items.push((
                        cf.line,
                        FlowNode {
                            kind,
                            label: cf.label,
                            file: sym.file.clone(),
                            line: cf.line,
                            col: cf.col,
                            signature: None,
                            symbol_id: None,
                            children: vec![],
                            truncated_reason: None,
                        },
                    ));
                }
            }
            Err(e) => tracing::warn!("flow: control-flow extraction failed: {}", e),
        }
    }

    // Merge call and cf items by line, then sort
    let mut all_items: Vec<(u32, FlowNode)> = call_items;
    all_items.extend(cf_items);
    all_items.sort_by_key(|(line, _)| *line);

    node.children = all_items.into_iter().map(|(_, n)| n).collect();

    visited.remove(&sym_id);
    Ok(node)
}
