use anyhow::Result;
use clap::Parser;
use gocx::cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::WARN
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(log_level.into()),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    match &cli.command {
        Commands::Init(args) => gocx::cli::init::run(args),
        Commands::Index(args) => gocx::cli::index::run(args),
        Commands::Status(args) => gocx::cli::status::run(args),
        Commands::Find(args) => gocx::cli::find::run(args),
        Commands::PkgTree(args) => gocx::cli::pkg_tree::run(args),
        // Phase 2
        Commands::Callers(args) => gocx::cli::callers::run(args),
        Commands::Callees(args) => gocx::cli::callees::run(args),
        Commands::Trace(args) => gocx::cli::trace::run(args),
        Commands::FindImpls(args) => gocx::cli::find_impls::run(args),
        Commands::FindIface(args) => gocx::cli::find_iface::run(args),
        Commands::Refs(args) => gocx::cli::refs::run(args),
        // Phase 3
        Commands::Impact(args) => gocx::cli::impact::run(args),
        Commands::Context(args) => gocx::cli::context::run(args),
    }
}
