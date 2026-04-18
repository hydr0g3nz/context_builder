use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::output;
use crate::semantic::call_graph;

#[derive(Args)]
pub struct CallersArgs {
    /// Symbol name (e.g. "Save", "UserService.Save")
    pub symbol: String,

    /// Path to Go repository root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// BFS depth limit
    #[arg(long, default_value = "2")]
    pub depth: usize,

    /// Output format
    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &CallersArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (store, sym) = resolve_symbol(&root, &args.symbol)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Results may be incomplete.", e);
                // Fall through to cached-only results
                let results: Vec<call_graph::CallNode> = vec![];
                emit_results(args, &sym.name, &results);
                return Ok(());
            }
        };

        let results = call_graph::callers(&store, &mut client, &sym, args.depth).await?;
        emit_results(args, &sym.name, &results);
        let _ = client.shutdown().await;
        Ok(())
    })
}

fn emit_results(args: &CallersArgs, query: &str, results: &[call_graph::CallNode]) {
    match args.output {
        OutputFormat::Json => {
            output::print_json(format!("callers {}", query), results);
        }
        OutputFormat::Text => {
            if results.is_empty() {
                println!("No callers found for {:?}", query);
                return;
            }
            println!("{:<40} {:<30} {}", "CALLER", "PACKAGE", "FILE:LINE");
            println!("{}", "-".repeat(90));
            for node in results {
                println!(
                    "{:<40} {:<30} {}:{}",
                    " ".repeat(node.depth * 2) + &node.symbol.name,
                    node.symbol.package,
                    node.symbol.file,
                    node.symbol.line
                );
            }
        }
    }
}
