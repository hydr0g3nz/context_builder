use anyhow::Result;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::cli::OutputFormat;
use crate::gopls::queries::uri_to_rel_path;
use crate::gopls::GoplsClient;
use crate::output;

#[derive(Args)]
pub struct RefsArgs {
    /// Symbol name
    pub symbol: String,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

#[derive(Serialize)]
pub struct RefLocation {
    pub file: String,
    pub line: u32,
    pub col: u32,
}

pub fn run(args: &RefsArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (_store, sym) = resolve_symbol(&root, &args.symbol)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Cannot find references.", e);
                match args.output {
                    OutputFormat::Json => {
                        output::print_json(
                            format!("refs {}", args.symbol),
                            &Vec::<RefLocation>::new(),
                        );
                    }
                    OutputFormat::Text => println!("gopls unavailable"),
                }
                return Ok(());
            }
        };

        let root_uri = client.root_uri.clone();
        let locs = client.references(&sym).await?;
        let refs: Vec<RefLocation> = locs
            .iter()
            .map(|loc| RefLocation {
                file: uri_to_rel_path(&loc.uri, &root_uri),
                line: loc.range.start.line + 1,
                col: loc.range.start.character + 1,
            })
            .collect();

        match args.output {
            OutputFormat::Json => {
                output::print_json(format!("refs {}", args.symbol), &refs);
            }
            OutputFormat::Text => {
                if refs.is_empty() {
                    println!("No references found for {:?}", args.symbol);
                } else {
                    println!("{}", "-".repeat(70));
                    for r in &refs {
                        println!("  {}:{}:{}", r.file, r.line, r.col);
                    }
                    println!("{} reference(s)", refs.len());
                }
            }
        }

        let _ = client.shutdown().await;
        Ok(())
    })
}
