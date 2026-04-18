use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol_kind;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::output;
use crate::semantic::impls;

#[derive(Args)]
pub struct FindIfaceArgs {
    /// Concrete type/struct name
    pub concrete: String,

    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &FindIfaceArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (store, concrete_sym) = resolve_symbol_kind(&root, &args.concrete, Some("struct"))?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}).", e);
                match args.output {
                    OutputFormat::Json => output::print_json(
                        format!("find-iface {}", args.concrete),
                        &Vec::<crate::model::Symbol>::new(),
                    ),
                    OutputFormat::Text => println!("gopls unavailable"),
                }
                return Ok(());
            }
        };

        let results =
            impls::find_interfaces_for(&store, &mut client, &concrete_sym).await?;

        match args.output {
            OutputFormat::Json => {
                output::print_json(format!("find-iface {}", args.concrete), &results);
            }
            OutputFormat::Text => {
                if results.is_empty() {
                    println!("No interfaces found for {:?}", args.concrete);
                } else {
                    println!("{:<40} {:<30} {}", "INTERFACE", "PACKAGE", "FILE:LINE");
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
