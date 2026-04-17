use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

use crate::cli::OutputFormat;
use crate::store::{symbols, Store};
use crate::output;

#[derive(Args)]
pub struct PkgTreeArgs {
    /// Path to Go repository root
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output format
    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

#[derive(Serialize)]
pub struct PackageSummary {
    pub package: String,
    pub counts: Vec<KindCount>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct KindCount {
    pub kind: String,
    pub count: i64,
}

pub fn run(args: &PkgTreeArgs) -> Result<()> {
    let root = args.path.canonicalize().with_context(|| {
        format!("Cannot resolve path: {}", args.path.display())
    })?;

    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No gocx index found. Run `gocx init && gocx index` first.");
    }

    let store = Store::open(&db_path)?;
    let packages = symbols::packages_with_symbols(&store.conn)?;

    let summaries: Vec<PackageSummary> = packages
        .into_iter()
        .map(|(pkg, kinds)| {
            let total: i64 = kinds.iter().map(|(_, c)| c).sum();
            PackageSummary {
                package: pkg,
                counts: kinds
                    .into_iter()
                    .map(|(k, c)| KindCount { kind: k, count: c })
                    .collect(),
                total,
            }
        })
        .collect();

    match args.output {
        OutputFormat::Json => {
            output::print_json("pkg-tree", &summaries);
        }
        OutputFormat::Text => {
            for pkg in &summaries {
                println!("{} ({})", pkg.package, pkg.total);
                for kc in &pkg.counts {
                    println!("  {:12} {}", kc.kind, kc.count);
                }
            }
        }
    }

    Ok(())
}
