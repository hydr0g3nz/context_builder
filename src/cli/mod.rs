pub mod find;
pub mod index;
pub mod init;
pub mod pkg_tree;
pub mod status;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "gocx",
    version,
    about = "AI-first Go codebase intelligence CLI",
    long_about = "gocx pre-indexes Go codebases into a semantic graph and exposes\ncompact, AI-optimized queries via CLI."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new gocx index in a Go repo
    Init(init::InitArgs),
    /// Build or update the symbol index
    Index(index::IndexArgs),
    /// Show index status and health
    Status(status::StatusArgs),
    /// Search for symbols by name
    Find(find::FindArgs),
    /// Show package tree structure
    #[command(name = "pkg-tree")]
    PkgTree(pkg_tree::PkgTreeArgs),
}
