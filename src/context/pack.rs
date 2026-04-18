/// Group ranked symbols by file path for the context bundle output.
use serde::Serialize;
use std::collections::HashMap;

use crate::context::rank::RankedNode;

#[derive(Debug, Serialize)]
pub struct PackedSymbol {
    pub name: String,
    pub kind: String,
    pub package: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub score: f64,
}

#[derive(Debug, Serialize)]
pub struct FileGroup {
    pub path: String,
    pub symbols: Vec<PackedSymbol>,
}

/// Pack ranked nodes into file-grouped output.
pub fn pack(ranked: Vec<RankedNode>) -> Vec<FileGroup> {
    let mut file_map: HashMap<String, Vec<PackedSymbol>> = HashMap::new();

    for rn in ranked {
        let sym = &rn.node.symbol;
        let packed = PackedSymbol {
            name: sym.name.clone(),
            kind: sym.kind.to_string(),
            package: sym.package.clone(),
            line: sym.line,
            signature: sym.signature.clone(),
            doc: sym.doc.clone(),
            score: (rn.score * 100.0).round() / 100.0,
        };
        file_map
            .entry(sym.file.clone())
            .or_default()
            .push(packed);
    }

    // Sort each file's symbols by score descending, then sort files by top score
    let mut groups: Vec<FileGroup> = file_map
        .into_iter()
        .map(|(path, mut symbols)| {
            symbols.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            FileGroup { path, symbols }
        })
        .collect();

    groups.sort_by(|a, b| {
        let a_top = a.symbols.first().map(|s| s.score).unwrap_or(0.0);
        let b_top = b.symbols.first().map(|s| s.score).unwrap_or(0.0);
        b_top.partial_cmp(&a_top).unwrap_or(std::cmp::Ordering::Equal)
    });

    groups
}

/// Build a human-readable summary line.
pub fn summarize(groups: &[FileGroup], seed_names: &[String]) -> String {
    let pkg_count = groups
        .iter()
        .flat_map(|g| g.symbols.iter().map(|s| s.package.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .len();
    let sym_count: usize = groups.iter().map(|g| g.symbols.len()).sum();
    let focus = groups.first().map(|g| g.path.as_str()).unwrap_or("unknown");

    let seeds_display = if seed_names.is_empty() {
        String::new()
    } else {
        format!(" seeding from {}", seed_names.join(", "))
    };

    format!(
        "Task touches {} package(s), {} symbols{}; focus on {}",
        pkg_count, sym_count, seeds_display, focus
    )
}
