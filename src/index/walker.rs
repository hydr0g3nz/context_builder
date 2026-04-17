use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub struct WalkConfig {
    pub root: PathBuf,
    pub include_tests: bool,
    pub include_vendor: bool,
}

impl WalkConfig {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            include_tests: false,
            include_vendor: false,
        }
    }
}

pub fn collect_go_files(config: &WalkConfig) -> Vec<PathBuf> {
    let mut builder = WalkBuilder::new(&config.root);
    builder
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .git_global(true);

    let include_tests = config.include_tests;
    let include_vendor = config.include_vendor;

    builder
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            // must be a .go file
            if path.extension().and_then(|e| e.to_str()) != Some("go") {
                return false;
            }
            let path_str = path.to_string_lossy();
            // skip vendor unless opted in
            if !include_vendor && path_str.contains("/vendor/") {
                return false;
            }
            // skip testdata
            if path_str.contains("/testdata/") {
                return false;
            }
            // skip _test.go unless opted in
            if !include_tests && path_str.ends_with("_test.go") {
                return false;
            }
            true
        })
        .map(|entry| entry.into_path())
        .collect()
}
