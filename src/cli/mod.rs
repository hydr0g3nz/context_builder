pub mod callers;
pub mod callees;
pub mod context;
pub mod find;
pub mod find_iface;
pub mod find_impls;
pub mod flow;
pub mod impact;
pub mod index;
pub mod init;
pub mod pkg_tree;
pub mod refs;
pub mod resolve;
pub mod status;
pub mod trace;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "gocx",
    version,
    about = "AI-first Go codebase intelligence CLI",
    long_about = "gocx pre-indexes Go codebases into a semantic graph and exposes\ncompact, AI-optimized queries via CLI."
)]
pub struct Cli {
    /// Enable verbose debug output with timing info (written to stderr)
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

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

    // ── Phase 2: Semantic commands ──────────────────────────────────────────
    /// Find all callers of a symbol (requires gopls)
    Callers(callers::CallersArgs),
    /// Find all callees of a symbol (requires gopls)
    Callees(callees::CalleesArgs),
    /// Trace a call path between two symbols (requires gopls)
    Trace(trace::TraceArgs),
    /// Find implementations of an interface (requires gopls)
    #[command(name = "find-impls")]
    FindImpls(find_impls::FindImplsArgs),
    /// Find interfaces satisfied by a concrete type (requires gopls)
    #[command(name = "find-iface")]
    FindIface(find_iface::FindIfaceArgs),
    /// Find all references to a symbol (requires gopls)
    Refs(refs::RefsArgs),

    // ── Phase 3: AI-native commands ─────────────────────────────────────────
    /// Analyze the blast radius of changing a symbol (requires gopls)
    Impact(impact::ImpactArgs),
    /// Build a ranked context bundle from a free-text task description
    Context(context::ContextArgs),
    /// Build an AI-readable call+control-flow tree from a root symbol (requires gopls)
    Flow(flow::FlowArgs),
}
