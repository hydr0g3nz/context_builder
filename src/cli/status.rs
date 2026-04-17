use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::store::{schema, symbols, files, Store};

#[derive(Args)]
pub struct StatusArgs {
    /// Path to Go repository root
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

pub fn run(args: &StatusArgs) -> Result<()> {
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

    let store = Store::open(&db_path)?;
    let conn = &store.conn;

    let schema_ver = schema::get_schema_version(conn).unwrap_or(0);
    let file_count = files::count_files(conn).unwrap_or(0);
    let kind_counts = symbols::count_symbols_by_kind(conn).unwrap_or_default();
    let total_symbols: i64 = kind_counts.iter().map(|(_, c)| c).sum();

    let last_indexed: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'last_full_index'",
            [],
            |row| row.get(0),
        )
        .ok()
        .and_then(|v: String| v.parse::<u64>().ok())
        .map(|ts| format!("{} UTC", humanize_timestamp(ts)));

    let db_size = std::fs::metadata(&db_path)
        .map(|m| format_bytes(m.len()))
        .unwrap_or_else(|_| "unknown".to_string());

    println!("gocx Status");
    println!("  Index path:    {}", db_path.display());
    println!("  Schema version: {}", schema_ver);
    println!("  Files indexed: {}", file_count);
    println!("  Total symbols: {}", total_symbols);
    println!("  DB size:       {}", db_size);
    if let Some(ts) = last_indexed {
        println!("  Last indexed:  {}", ts);
    }
    println!();
    println!("Symbols by kind:");
    for (kind, count) in &kind_counts {
        println!("  {:12} {}", kind, count);
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn humanize_timestamp(secs: u64) -> String {
    // Simple: just return ISO-like format using manual calculation
    // Avoiding chrono dependency in Phase 1
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Approx date from epoch — good enough for status display
    let years = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", years, month, day, h, m, s)
}
