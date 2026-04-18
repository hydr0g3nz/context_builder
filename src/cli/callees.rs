use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::output;
use crate::semantic::call_graph;

#[derive(Args)]
pub struct CalleesArgs {
    /// Symbol name
    pub symbol: String,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    #[arg(long, default_value = "2")]
    pub depth: usize,

    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &CalleesArgs) -> Result<()> {
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
                let results: Vec<call_graph::CallNode> = vec![];
                emit_results(args, &sym.name, &results);
                return Ok(());
            }
        };

        let results = call_graph::callees(&store, &mut client, &sym, args.depth).await?;
        emit_results(args, &sym.name, &results);
        let _ = client.shutdown().await;
        Ok(())
    })
}

fn emit_results(args: &CalleesArgs, query: &str, results: &[call_graph::CallNode]) {
    match args.output {
        OutputFormat::Json => {
            output::print_json(format!("callees {}", query), results);
        }
        OutputFormat::Text => {
            if results.is_empty() {
                println!("No callees found for {:?}", query);
                return;
            }
            println!("{:<40} {:<30} FILE:LINE", "CALLEE", "PACKAGE");
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
