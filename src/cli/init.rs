use anyhow::{Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

use crate::store::Store;

#[derive(Args)]
pub struct InitArgs {
    /// Path to Go repository root (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

pub fn run(args: &InitArgs) -> Result<()> {
    let root = args.path.canonicalize().with_context(|| {
        format!("Cannot resolve path: {}", args.path.display())
    })?;

    let gocx_dir = root.join(".gocx");
    let db_path = gocx_dir.join("index.db");
    let gitignore_path = gocx_dir.join(".gitignore");
    let config_path = gocx_dir.join("config.toml");

    if gocx_dir.exists() && db_path.exists() {
        eprintln!("gocx already initialized at {}", root.display());
        let store = Store::open(&db_path)?;
        let ver = crate::store::schema::get_schema_version(&store.conn)
            .unwrap_or(0);
        eprintln!("Schema version: {}", ver);
        return Ok(());
    }

    std::fs::create_dir_all(&gocx_dir)
        .with_context(|| format!("Cannot create .gocx/ at {}", root.display()))?;

    // create .gitignore so the index doesn't leak into git
    std::fs::write(&gitignore_path, "*\n")?;

    // create and migrate the database
    Store::open_or_create(&db_path).context("Failed to initialize database")?;

    // detect go.mod module name
    let module = detect_module_name(&root);

    let config = format!(
        "# gocx configuration\nmodule = \"{}\"\nroot = \"{}\"\n",
        module,
        root.display()
    );
    std::fs::write(&config_path, config)?;

    println!("Initialized gocx index at {}", root.display());
    if let Some(m) = &module.is_empty().then_some("(unknown)").or(Some(&module)) {
        println!("Module: {}", m);
    }
    println!("Run `gocx index` to build the symbol index.");
    Ok(())
}

fn detect_module_name(root: &Path) -> String {
    let go_mod = root.join("go.mod");
    if let Ok(content) = std::fs::read_to_string(go_mod) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("module ") {
                return rest.trim().to_string();
            }
        }
    }
    String::new()
}
