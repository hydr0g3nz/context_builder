use anyhow::Result;
use serde::Serialize;

use crate::model::{Symbol, SymbolKind};
use crate::semantic::call_graph::{self, CallNode};
use crate::store::Store;
use crate::gopls::GoplsClient;

#[derive(Debug, Serialize)]
pub struct ImpactReport {
    pub symbol: Symbol,
    pub direct_callers: Vec<CallNode>,
    pub transitive_reach: usize,
    pub risk_signals: Vec<String>,
    pub breakable_tests: Vec<CallNode>,
}

pub async fn run(
    store: &Store,
    client: &mut GoplsClient,
    sym: &Symbol,
    depth: usize,
) -> Result<ImpactReport> {
    let all_callers = call_graph::callers(store, client, sym, depth).await?;

    let direct_callers: Vec<CallNode> = all_callers
        .iter()
        .filter(|n| n.depth == 1)
        .cloned()
        .collect();

    let breakable_tests: Vec<CallNode> = all_callers
        .iter()
        .filter(|n| n.symbol.file.ends_with("_test.go"))
        .cloned()
        .collect();

    let transitive_reach = all_callers.len();

    let risk_signals = compute_risk_signals(sym, &all_callers, transitive_reach, &breakable_tests);

    Ok(ImpactReport {
        symbol: sym.clone(),
        direct_callers,
        transitive_reach,
        risk_signals,
        breakable_tests,
    })
}

fn compute_risk_signals(
    _sym: &Symbol,
    all_callers: &[CallNode],
    transitive_reach: usize,
    breakable_tests: &[CallNode],
) -> Vec<String> {
    let mut signals = Vec::new();

    let http_handler = all_callers.iter().find(|n| {
        let name_lc = n.symbol.name.to_lowercase();
        let file_lc = n.symbol.file.to_lowercase();
        name_lc.contains("servehttp")
            || name_lc.contains("handler")
            || name_lc.contains("handlefunc")
            || file_lc.contains("handler")
    });

    if let Some(handler) = http_handler {
        signals.push(format!(
            "called from HTTP handler ({})",
            handler.symbol.name
        ));
    }

    if transitive_reach > 10 {
        signals.push(format!("high fan-in ({} callers)", transitive_reach));
    }

    if !breakable_tests.is_empty() {
        signals.push(format!(
            "breaks {} test(s) if changed",
            breakable_tests.len()
        ));
    }

    signals
}

/// Build next_actions hints for impact results.
pub fn next_actions(sym: &Symbol, report: &ImpactReport) -> Vec<String> {
    let mut hints = Vec::new();

    // If an HTTP handler was found in callers, suggest tracing from it
    let http_handler = report.direct_callers.iter().find(|n| {
        let name_lc = n.symbol.name.to_lowercase();
        let file_lc = n.symbol.file.to_lowercase();
        name_lc.contains("servehttp")
            || name_lc.contains("handler")
            || name_lc.contains("handlefunc")
            || file_lc.contains("handler")
    });
    if let Some(handler) = http_handler {
        hints.push(format!("gocx trace {} {}", handler.symbol.name, sym.name));
    }

    // If sym is a method, suggest finding interfaces its receiver implements
    if sym.kind == SymbolKind::Method {
        let receiver = sym.name.split('.').next().unwrap_or(&sym.name);
        hints.push(format!("gocx find-iface {}", receiver));
    }

    hints
}
