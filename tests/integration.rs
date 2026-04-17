use std::path::Path;
use tempfile::TempDir;

fn fixture_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").as_path();
    // Note: use a static approach via CARGO_MANIFEST_DIR
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

#[test]
fn test_index_fixtures() {
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));

    // Create a temp dir as our fake "repo root" with .gocx/
    let tmp = TempDir::new().unwrap();
    let gocx_dir = tmp.path().join(".gocx");
    std::fs::create_dir_all(&gocx_dir).unwrap();

    // Copy fixture files to temp dir so relative paths work
    let dest = tmp.path().join("pkg").join("userservice");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::copy(
        fixtures.join("simple.go"),
        dest.join("simple.go"),
    )
    .unwrap();

    let db_path = gocx_dir.join("index.db");
    let mut store = gocx::store::Store::open_or_create(&db_path).unwrap();

    let stats = gocx::index::index_full(tmp.path(), &mut store.conn, false).unwrap();

    assert!(stats.files_parsed >= 1, "should have parsed at least 1 file");
    assert!(stats.symbols_extracted >= 5, "should have extracted at least 5 symbols");

    // verify specific symbols
    let q = gocx::store::symbols::FindQuery {
        query: "UserService",
        exact: false,
        kind: None,
        package: None,
        limit: 20,
    };
    let results = gocx::store::symbols::find_symbols(&store.conn, &q).unwrap();
    let names: Vec<_> = results.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"UserService"), "should find UserService struct");
    assert!(
        names.iter().any(|n| n.contains("UserService.Save")),
        "should find UserService.Save method"
    );

    // verify interface
    let iq = gocx::store::symbols::FindQuery {
        query: "UserStore",
        exact: true,
        kind: None,
        package: None,
        limit: 5,
    };
    let iresults = gocx::store::symbols::find_symbols(&store.conn, &iq).unwrap();
    assert!(!iresults.is_empty(), "should find UserStore interface");
    assert_eq!(iresults[0].kind, gocx::model::SymbolKind::Interface);

    // verify kind counts
    let counts = gocx::store::symbols::count_symbols_by_kind(&store.conn).unwrap();
    let kinds: Vec<_> = counts.iter().map(|(k, _)| k.as_str()).collect();
    assert!(kinds.contains(&"struct"), "should have struct kind");
    assert!(kinds.contains(&"method"), "should have method kind");
    assert!(kinds.contains(&"interface"), "should have interface kind");
}

#[test]
fn test_pkg_tree() {
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    let tmp = TempDir::new().unwrap();
    let gocx_dir = tmp.path().join(".gocx");
    std::fs::create_dir_all(&gocx_dir).unwrap();

    let dest = tmp.path().join("userservice");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::copy(fixtures.join("simple.go"), dest.join("simple.go")).unwrap();

    let db_path = gocx_dir.join("index.db");
    let mut store = gocx::store::Store::open_or_create(&db_path).unwrap();
    gocx::index::index_full(tmp.path(), &mut store.conn, false).unwrap();

    let packages = gocx::store::symbols::packages_with_symbols(&store.conn).unwrap();
    assert!(!packages.is_empty(), "should have at least one package");
    assert!(
        packages.iter().any(|(pkg, _)| pkg == "userservice"),
        "should find userservice package"
    );
}
