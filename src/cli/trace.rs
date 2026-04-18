use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::model::Symbol;
use crate::output;
use crate::semantic::call_graph;

#[derive(Args)]
pub struct TraceArgs {
    /// Starting symbol
    pub from: String,

    /// Target symbol
    pub to: String,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    #[arg(long, default_value = "8")]
    pub max_depth: usize,

    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &TraceArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (store, from_sym) = resolve_symbol(&root, &args.from)?;
    let (_, to_sym) = resolve_symbol(&root, &args.to)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Cannot trace path.", e);
                emit_path(args, &args.from, &[], false);
                return Ok(());
            }
        };

        let path = call_graph::trace_path(&store, &mut client, &from_sym, &to_sym, args.max_depth)
            .await?;
        let found = !path.is_empty();
        emit_path(args, &format!("{} → {}", args.from, args.to), &path, found);
        let _ = client.shutdown().await;
        Ok(())
    })
}

fn emit_path(args: &TraceArgs, query: &str, path: &[Symbol], found: bool) {
    match args.output {
        OutputFormat::Json => {
            output::print_json(format!("trace {}", query), &path);
        }
        OutputFormat::Text => {
            if !found || path.is_empty() {
                println!("No path found from {:?} to {:?}", args.from, args.to);
                return;
            }
            println!("Call path ({} hops):", path.len() - 1);
            for (i, sym) in path.iter().enumerate() {
                let arrow = if i == 0 { "  " } else { "→ " };
                println!("  {}{} ({}:{})", arrow, sym.name, sym.file, sym.line);
            }
        }
    }
}
