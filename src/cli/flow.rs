use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::flow::render::render_text;
use crate::flow::tree::{build_flow, FlowOptions};
use crate::gopls::GoplsClient;
use crate::output;

#[derive(Args)]
pub struct FlowArgs {
    /// Root symbol (e.g. "main", "Server.Run", "UserService.Save")
    pub root: String,

    /// Max call-tree depth
    #[arg(long, default_value = "3")]
    pub depth: usize,

    /// Exclude paths matching these substrings (repeatable; e.g. --exclude vendor)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Emit JSON envelope instead of indented text
    #[arg(long)]
    pub json: bool,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,
}

pub fn run(args: &FlowArgs) -> Result<()> {
    let root_path = args.path.canonicalize()?;
    let (store, root_sym) = resolve_symbol(&root_path, &args.root)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root_path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Flow requires gopls.", e);
                return Ok(());
            }
        };

        let opts = FlowOptions {
            max_depth: args.depth,
            exclude_patterns: args.exclude.clone(),
        };

        let tree = build_flow(&store, &mut client, &root_sym, &opts).await?;
        let _ = client.shutdown().await;

        if args.json {
            output::print_json(format!("flow {}", args.root), &tree);
        } else {
            let text = render_text(&tree);
            print!("{}", text);
        }

        Ok(())
    })
}
