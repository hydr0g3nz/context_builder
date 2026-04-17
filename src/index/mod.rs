pub mod extractor;
pub mod parser;
pub mod walker;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;
use tracing::info;

use crate::model::{FileRecord, Symbol};
use crate::store::{files, symbols};

pub struct IndexStats {
    pub files_parsed: usize,
    pub symbols_extracted: usize,
    pub elapsed_ms: u128,
}

enum WorkerMsg {
    Result(FileRecord, Vec<Symbol>),
    Error(String),
}

pub fn index_full(root: &Path, conn: &mut Connection, include_tests: bool) -> Result<IndexStats> {
    let start = std::time::Instant::now();

    // truncate existing data
    symbols::truncate_symbols(conn)?;

    let config = walker::WalkConfig {
        root: root.to_path_buf(),
        include_tests,
        include_vendor: false,
    };

    let go_files = walker::collect_go_files(&config);
    let total = go_files.len();

    info!("Found {} Go files to index", total);

    let pb = if atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} files  {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    let (tx, rx) = mpsc::channel::<WorkerMsg>();

    // Parse files in parallel using rayon
    let root_clone = root.to_path_buf();
    rayon::spawn(move || {
        go_files.par_iter().for_each(|path| {
            let result = parser::parse_file(path, &root_clone);
            let msg = match result {
                Ok(pr) => WorkerMsg::Result(pr.file, pr.symbols),
                Err(e) => WorkerMsg::Error(format!("{}: {}", path.display(), e)),
            };
            let _ = tx.send(msg);
        });
    });

    let mut files_parsed = 0usize;
    let mut symbols_extracted = 0usize;
    let mut pending_symbols: Vec<Symbol> = Vec::with_capacity(1024);

    for msg in rx {
        match msg {
            WorkerMsg::Result(file_rec, syms) => {
                if let Err(e) = files::upsert_file(conn, &file_rec) {
                    tracing::warn!("Failed to upsert file {}: {}", file_rec.path, e);
                }
                symbols_extracted += syms.len();
                pending_symbols.extend(syms);

                // flush in batches of 500
                if pending_symbols.len() >= 500 {
                    if let Err(e) = symbols::insert_symbols_batch(conn, &pending_symbols) {
                        tracing::warn!("Batch insert error: {}", e);
                    }
                    pending_symbols.clear();
                }

                files_parsed += 1;
                if let Some(ref pb) = pb {
                    pb.inc(1);
                    pb.set_message(format!("{} symbols", symbols_extracted));
                }
            }
            WorkerMsg::Error(e) => {
                tracing::warn!("Parse error: {}", e);
                if let Some(ref pb) = pb {
                    pb.inc(1);
                }
            }
        }
    }

    // flush remainder
    if !pending_symbols.is_empty() {
        symbols::insert_symbols_batch(conn, &pending_symbols)?;
    }

    if let Some(pb) = pb {
        pb.finish_with_message("done");
    }

    let elapsed_ms = start.elapsed().as_millis();
    Ok(IndexStats {
        files_parsed,
        symbols_extracted,
        elapsed_ms,
    })
}

pub fn index_incremental(root: &Path, conn: &mut Connection, include_tests: bool) -> Result<IndexStats> {
    let start = std::time::Instant::now();

    let config = walker::WalkConfig {
        root: root.to_path_buf(),
        include_tests,
        include_vendor: false,
    };

    let go_files = walker::collect_go_files(&config);
    let total_found = go_files.len();
    info!("Found {} Go files, checking for changes...", total_found);

    // determine which files need re-indexing
    let to_reindex: Vec<_> = go_files
        .iter()
        .filter(|path| {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            let hash = blake3::hash(source.as_bytes()).to_hex().to_string();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            match files::get_file(conn, &rel) {
                Ok(Some(rec)) => rec.hash != hash,
                _ => true,
            }
        })
        .cloned()
        .collect();

    info!("{} files need re-indexing", to_reindex.len());

    if to_reindex.is_empty() {
        return Ok(IndexStats {
            files_parsed: 0,
            symbols_extracted: 0,
            elapsed_ms: start.elapsed().as_millis(),
        });
    }

    let (tx, rx) = mpsc::channel::<WorkerMsg>();
    let root_clone = root.to_path_buf();

    rayon::spawn(move || {
        to_reindex.par_iter().for_each(|path| {
            let msg = match parser::parse_file(path, &root_clone) {
                Ok(pr) => WorkerMsg::Result(pr.file, pr.symbols),
                Err(e) => WorkerMsg::Error(format!("{}: {}", path.display(), e)),
            };
            let _ = tx.send(msg);
        });
    });

    let mut files_parsed = 0usize;
    let mut symbols_extracted = 0usize;

    for msg in rx {
        match msg {
            WorkerMsg::Result(file_rec, syms) => {
                // remove old symbols for this file first
                let _ = files::delete_file_symbols(conn, &file_rec.path);
                let _ = files::upsert_file(conn, &file_rec);
                symbols_extracted += syms.len();
                if !syms.is_empty() {
                    let _ = symbols::insert_symbols_batch(conn, &syms);
                }
                files_parsed += 1;
            }
            WorkerMsg::Error(e) => tracing::warn!("Parse error: {}", e),
        }
    }

    Ok(IndexStats {
        files_parsed,
        symbols_extracted,
        elapsed_ms: start.elapsed().as_millis(),
    })
}
