/// Extract seed symbols from a free-text task description.
use anyhow::Result;
use std::collections::HashMap;

use crate::model::Symbol;
use crate::store::symbols::{find_symbols, FindQuery};
use rusqlite::Connection;

/// Extract seed symbols from a task description string.
/// Returns deduplicated symbols ordered by relevance.
pub fn extract_seeds(conn: &Connection, task: &str, limit_per_candidate: usize) -> Result<Vec<Symbol>> {
    let candidates = extract_candidates(task);

    let mut seen_ids: HashMap<i64, ()> = HashMap::new();
    let mut seeds: Vec<Symbol> = Vec::new();

    for candidate in candidates {
        let q = FindQuery {
            query: &candidate,
            exact: false,
            kind: None,
            package: None,
            limit: limit_per_candidate,
        };
        let results = find_symbols(conn, &q)?;
        for sym in results {
            if let Some(id) = sym.id {
                if seen_ids.insert(id, ()).is_none() {
                    seeds.push(sym);
                }
            }
        }
    }

    Ok(seeds)
}

/// Extract Go identifier candidates from free text.
/// Looks for CamelCase words and dotted paths (e.g. "UserService.Save").
fn extract_candidates(text: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    // Pass 1: find CamelCase tokens or dotted CamelCase paths
    for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"' || c == '\'') {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
        if token.is_empty() {
            continue;
        }
        // Accept if it starts with uppercase (Go exported identifier)
        if token.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            candidates.push(token.to_string());
            // Also try just the part before the dot (receiver type)
            if let Some(dot) = token.find('.') {
                candidates.push(token[..dot].to_string());
            }
            continue;
        }
        // Pass 2: lowercase words ≥ 4 chars as fuzzy fallback
        if token.len() >= 4 && token.chars().all(|c| c.is_alphabetic()) {
            candidates.push(token.to_string());
        }
    }

    // Deduplicate preserving order
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}
