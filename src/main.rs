use anyhow::Result;
use clap::Parser;
use gocx::cli::{Cli, Commands};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init(args) => gocx::cli::init::run(args),
        Commands::Index(args) => gocx::cli::index::run(args),
        Commands::Status(args) => gocx::cli::status::run(args),
        Commands::Find(args) => gocx::cli::find::run(args),
        Commands::PkgTree(args) => gocx::cli::pkg_tree::run(args),
    }
}
