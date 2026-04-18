use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

use crate::cli::OutputFormat;
use crate::store::{symbols, Store};
use crate::output;

#[derive(Args)]
pub struct FindArgs {
    /// Search query (substring match by default)
    pub query: String,

    /// Path to Go repository root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Match exactly (case-sensitive)
    #[arg(long)]
    pub exact: bool,

    /// Filter by symbol kind (func, method, struct, interface, type_alias, const, var)
    #[arg(long)]
    pub kind: Option<String>,

    /// Filter by package name
    #[arg(long)]
    pub package: Option<String>,

    /// Maximum results to return
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Output format
    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

#[derive(Serialize)]
pub struct SymbolResult {
    pub name: String,
    pub kind: String,
    pub package: String,
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub visibility: String,
}

pub fn run(args: &FindArgs) -> Result<()> {
    let root = args.path.canonicalize().with_context(|| {
        format!("Cannot resolve path: {}", args.path.display())
    })?;

    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No gocx index found. Run `gocx init && gocx index` first.");
    }

    let store = Store::open(&db_path)?;

    let q = symbols::FindQuery {
        query: &args.query,
        exact: args.exact,
        kind: args.kind.as_deref(),
        package: args.package.as_deref(),
        limit: args.limit + 1, // fetch one extra to detect truncation
    };

    let mut results = symbols::find_symbols(&store.conn, &q)?;
    let truncated = results.len() > args.limit;
    if truncated {
        results.truncate(args.limit);
    }

    let result_vec: Vec<SymbolResult> = results
        .into_iter()
        .map(|s| SymbolResult {
            name: s.name,
            kind: s.kind.to_string(),
            package: s.package,
            file: s.file,
            line: s.line,
            signature: s.signature,
            doc: s.doc,
            visibility: s.visibility.as_str().to_string(),
        })
        .collect();

    match args.output {
        OutputFormat::Json => {
            output::print_json_truncated(
                format!("find {}", args.query),
                &result_vec,
                truncated,
            );
        }
        OutputFormat::Text => {
            if result_vec.is_empty() {
                println!("No symbols found for {:?}", args.query);
            } else {
                println!("{:<40} {:<12} {:<30} FILE:LINE", "NAME", "KIND", "PACKAGE");
                println!("{}", "-".repeat(100));
                for sym in &result_vec {
                    println!(
                        "{:<40} {:<12} {:<30} {}:{}",
                        sym.name, sym.kind, sym.package, sym.file, sym.line
                    );
                }
                if truncated {
                    println!("... (truncated, use --limit to see more)");
                }
            }
        }
    }

    Ok(())
}
