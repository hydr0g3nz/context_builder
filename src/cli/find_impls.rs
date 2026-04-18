use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol_kind;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::output;
use crate::semantic::impls;

#[derive(Args)]
pub struct FindImplsArgs {
    /// Interface symbol name
    pub interface: String,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &FindImplsArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (store, iface_sym) = resolve_symbol_kind(&root, &args.interface, Some("interface"))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Results may be incomplete.", e);
                match args.output {
                    OutputFormat::Json => output::print_json(
                        format!("find-impls {}", args.interface),
                        &Vec::<crate::model::Symbol>::new(),
                    ),
                    OutputFormat::Text => println!("gopls unavailable"),
                }
                return Ok(());
            }
        };

        let results =
            impls::find_implementations(&store, &mut client, &iface_sym).await?;

        match args.output {
            OutputFormat::Json => {
                output::print_json(format!("find-impls {}", args.interface), &results);
            }
            OutputFormat::Text => {
                if results.is_empty() {
                    println!("No implementations found for {:?}", args.interface);
                } else {
                    println!("{:<40} {:<30} {}", "IMPL", "PACKAGE", "FILE:LINE");
                    println!("{}", "-".repeat(90));
                    for sym in &results {
                        println!(
                            "{:<40} {:<30} {}:{}",
                            sym.name, sym.package, sym.file, sym.line
                        );
                    }
                }
            }
        }

        let _ = client.shutdown().await;
        Ok(())
    })
}
