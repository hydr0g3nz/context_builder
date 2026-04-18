use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::cli::resolve::resolve_symbol;
use crate::cli::OutputFormat;
use crate::gopls::GoplsClient;
use crate::impact;
use crate::output::ResponseEnvelope;

#[derive(Args)]
pub struct ImpactArgs {
    /// Symbol name (e.g. "Save", "UserService.Save")
    pub symbol: String,

    /// Path to Go repository root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// BFS depth limit for caller traversal
    #[arg(long, default_value = "3")]
    pub depth: usize,

    /// Output format
    #[arg(long, default_value = "json")]
    pub output: OutputFormat,
}

pub fn run(args: &ImpactArgs) -> Result<()> {
    let root = args.path.canonicalize()?;
    let (store, sym) = resolve_symbol(&root, &args.symbol)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut client = match GoplsClient::new(&root).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: gopls unavailable ({}). Results will be limited to cached edges.", e);
                // Emit empty report
                let report = impact::ImpactReport {
                    symbol: sym.clone(),
                    direct_callers: vec![],
                    transitive_reach: 0,
                    risk_signals: vec![],
                    breakable_tests: vec![],
                };
                emit_results(args, &sym.name, report, vec![]);
                return Ok(());
            }
        };

        let report = impact::run(&store, &mut client, &sym, args.depth).await?;
        let hints = impact::next_actions(&sym, &report);
        emit_results(args, &sym.name, report, hints);

        let _ = client.shutdown().await;
        Ok(())
    })
}

fn emit_results(args: &ImpactArgs, query: &str, report: impact::ImpactReport, next_actions: Vec<String>) {
    match args.output {
        OutputFormat::Json => {
            let mut env = ResponseEnvelope::new(format!("impact {}", query), &report);
            env.next_actions = next_actions;
            env.print_json();
        }
        OutputFormat::Text => {
            println!("Symbol: {} ({})", report.symbol.name, report.symbol.kind);
            println!("Transitive reach: {} callers", report.transitive_reach);

            if !report.risk_signals.is_empty() {
                println!("\nRisk signals:");
                for s in &report.risk_signals {
                    println!("  ! {}", s);
                }
            }

            if !report.direct_callers.is_empty() {
                println!("\nDirect callers:");
                for n in &report.direct_callers {
                    println!(
                        "  {:<40} {:<30} {}:{}",
                        n.symbol.name, n.symbol.package, n.symbol.file, n.symbol.line
                    );
                }
            }

            if !report.breakable_tests.is_empty() {
                println!("\nBreakable tests:");
                for n in &report.breakable_tests {
                    println!("  {}", n.symbol.name);
                }
            }

            if !next_actions.is_empty() {
                println!("\nNext actions:");
                for hint in &next_actions {
                    println!("  $ {}", hint);
                }
            }
        }
    }
}
