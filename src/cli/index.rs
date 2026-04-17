use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::index;
use crate::store::Store;

#[derive(Args)]
pub struct IndexArgs {
    /// Path to Go repository root
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Full re-index (truncates existing data)
    #[arg(long, conflicts_with = "incremental")]
    pub full: bool,

    /// Incremental: only re-parse changed files
    #[arg(long, conflicts_with = "full")]
    pub incremental: bool,

    /// Include test files (_test.go)
    #[arg(long)]
    pub include_tests: bool,
}

pub fn run(args: &IndexArgs) -> Result<()> {
    let root = args.path.canonicalize().with_context(|| {
        format!("Cannot resolve path: {}", args.path.display())
    })?;

    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        anyhow::bail!(
            "No gocx index found at {}. Run `gocx init` first.",
            root.display()
        );
    }

    let mut store = Store::open(&db_path)?;

    let use_incremental = args.incremental && !args.full;

    let stats = if use_incremental {
        eprintln!("Running incremental index...");
        index::index_incremental(&root, &mut store.conn, args.include_tests)?
    } else {
        eprintln!("Running full index...");
        index::index_full(&root, &mut store.conn, args.include_tests)?
    };

    // update last index timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    store.conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_full_index', ?1)",
        rusqlite::params![now.to_string()],
    )?;

    println!(
        "Indexed {} files, {} symbols in {:.1}s",
        stats.files_parsed,
        stats.symbols_extracted,
        stats.elapsed_ms as f64 / 1000.0
    );

    Ok(())
}
