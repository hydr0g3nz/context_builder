use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::cli::OutputFormat;
use crate::context;
use crate::output::ResponseEnvelope;
use crate::store::Store;

#[derive(Args)]
pub struct ContextArgs {
    /// Free-text task description (e.g. "add JWT auth to UserService")
    pub task: String,

    /// Path to Go repository root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Maximum number of symbols to include
    #[arg(long, default_value = "30")]
    pub limit: usize,

    /// Output format
    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &ContextArgs) -> Result<()> {
    let root = args.path.canonicalize().with_context(|| {
        format!("Cannot resolve path: {}", args.path.display())
    })?;

    let db_path = root.join(".gocx").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No gocx index found. Run `gocx init && gocx index` first.");
    }

    let store = Store::open(&db_path)?;
    let bundle = context::build(&store, &args.task, args.limit)?;
    let hints = context::next_actions(&bundle);

    match args.output {
        OutputFormat::Json => {
            let mut env = ResponseEnvelope::new(format!("context {}", args.task), &bundle);
            env.next_actions = hints;
            env.print_json();
        }
        OutputFormat::Text => {
            println!("{}", bundle.summary);
            println!();
            if bundle.seeds.is_empty() {
                println!("No matching symbols found for the task description.");
                return Ok(());
            }
            println!("Seeds: {}", bundle.seeds.join(", "));
            println!();
            for group in &bundle.files {
                println!("{}:", group.path);
                for sym in &group.symbols {
                    let sig = sym.signature.as_deref().unwrap_or("");
                    println!(
                        "  {:>4}  {:<12}  {:<40}  score={:.2}",
                        sym.line, sym.kind, sym.name, sym.score
                    );
                    if !sig.is_empty() {
                        println!("        sig: {}", sig);
                    }
                }
                println!();
            }
            if !hints.is_empty() {
                println!("Next actions:");
                for h in &hints {
                    println!("  $ {}", h);
                }
            }
        }
    }

    Ok(())
}
