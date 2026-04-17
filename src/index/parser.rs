use anyhow::Result;
use std::path::Path;
use std::time::SystemTime;

use crate::model::{FileRecord, Symbol};
use super::extractor::GoExtractor;

pub struct ParseResult {
    pub file: FileRecord,
    pub symbols: Vec<Symbol>,
}

pub fn parse_file(path: &Path, root: &Path) -> Result<ParseResult> {
    let source = std::fs::read_to_string(path)?;
    let hash = blake3::hash(source.as_bytes()).to_hex().to_string();

    let mtime = path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let rel_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    let mut extractor = GoExtractor::new()?;
    let mut symbols = extractor.extract(&source, &rel_path)?;

    let package = symbols
        .iter()
        .find(|s| !s.package.is_empty())
        .map(|s| s.package.clone());

    // stamp hash into each symbol
    for sym in &mut symbols {
        sym.hash = Some(hash.clone());
    }

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(ParseResult {
        file: FileRecord {
            path: rel_path,
            hash,
            mtime,
            parsed_at: now,
            package,
        },
        symbols,
    })
}
