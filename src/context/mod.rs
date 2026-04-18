pub mod expand;
pub mod pack;
pub mod rank;
pub mod seed;

use anyhow::Result;
use serde::Serialize;

use crate::store::Store;

pub use pack::FileGroup;

#[derive(Debug, Serialize)]
pub struct ContextBundle {
    pub seeds: Vec<String>,
    pub files: Vec<FileGroup>,
    pub summary: String,
}

/// Build a context bundle from a free-text task description.
pub fn build(store: &Store, task: &str, limit: usize) -> Result<ContextBundle> {
    let seeds = seed::extract_seeds(&store.conn, task, 3)?;
    let seed_names: Vec<String> = seeds.iter().map(|s| s.name.clone()).collect();

    tracing::debug!("context: {} seeds from task {:?}", seeds.len(), task);

    let cap = (limit * 3).max(30);
    let expanded = expand::expand(&store.conn, &seeds, cap)?;

    tracing::debug!("context: {} expanded nodes", expanded.len());

    let ranked = rank::rank(expanded, limit);
    let files = pack::pack(ranked);
    let summary = pack::summarize(&files, &seed_names);

    Ok(ContextBundle {
        seeds: seed_names,
        files,
        summary,
    })
}

/// Build next_actions hints for a context bundle.
pub fn next_actions(bundle: &ContextBundle) -> Vec<String> {
    let mut hints = Vec::new();

    // Suggest digging into the top-ranked symbols
    for group in bundle.files.iter().take(1) {
        for sym in group.symbols.iter().take(2) {
            if sym.kind == "method" || sym.kind == "func" {
                hints.push(format!("gocx callers {}", sym.name));
                hints.push(format!("gocx impact {}", sym.name));
                break;
            }
        }
    }

    hints.truncate(3);
    hints
}
