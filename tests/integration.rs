use std::path::Path;
use tempfile::TempDir;

fn fixture_path() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

#[test]
fn test_index_fixtures() {
    let fixtures = fixture_path();

    let tmp = TempDir::new().unwrap();
    let gocx_dir = tmp.path().join(".gocx");
    std::fs::create_dir_all(&gocx_dir).unwrap();

    let dest = tmp.path().join("pkg").join("userservice");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::copy(fixtures.join("simple.go"), dest.join("simple.go")).unwrap();

    let db_path = gocx_dir.join("index.db");
    let mut store = gocx::store::Store::open_or_create(&db_path).unwrap();

    let stats = gocx::index::index_full(tmp.path(), &mut store.conn, false).unwrap();

    assert!(stats.files_parsed >= 1, "should have parsed at least 1 file");
    assert!(stats.symbols_extracted >= 5, "should have extracted at least 5 symbols");

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

    let counts = gocx::store::symbols::count_symbols_by_kind(&store.conn).unwrap();
    let kinds: Vec<_> = counts.iter().map(|(k, _)| k.as_str()).collect();
    assert!(kinds.contains(&"struct"), "should have struct kind");
    assert!(kinds.contains(&"method"), "should have method kind");
    assert!(kinds.contains(&"interface"), "should have interface kind");
}

#[test]
fn test_pkg_tree() {
    let fixtures = fixture_path();
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

#[test]
fn test_schema_v2_migration() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");
    let store = gocx::store::Store::open_or_create(&db_path).unwrap();

    // Verify schema version is 2
    let version = gocx::store::schema::get_schema_version(&store.conn);
    assert_eq!(version, Some(2), "schema should be at version 2");

    // Verify edges table exists
    let count: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);

    // Verify edge_resolution table exists
    let count: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM edge_resolution", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_edge_crud() {
    use gocx::store::edges::{is_resolved, mark_resolved, upsert_edge, Edge, EdgeKind};
    use gocx::store::symbols::insert_symbol;

    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("index.db");
    let store = gocx::store::Store::open_or_create(&db_path).unwrap();

    // Insert two dummy symbols
    let sym1 = gocx::model::Symbol {
        id: None,
        kind: gocx::model::SymbolKind::Func,
        name: "Alpha".to_string(),
        package: "pkg".to_string(),
        file: "a.go".to_string(),
        line: 1,
        col: 0,
        signature: None,
        doc: None,
        visibility: gocx::model::Visibility::Exported,
        hash: None,
    };
    let sym2 = gocx::model::Symbol {
        name: "Beta".to_string(),
        line: 10,
        ..sym1.clone()
    };

    let id1 = insert_symbol(&store.conn, &sym1).unwrap();
    let id2 = insert_symbol(&store.conn, &sym2).unwrap();

    // Upsert a CALLS edge
    upsert_edge(
        &store.conn,
        &Edge {
            src: id1,
            dst: id2,
            kind: EdgeKind::Calls,
            meta: None,
        },
    )
    .unwrap();

    // Idempotent upsert
    upsert_edge(
        &store.conn,
        &Edge {
            src: id1,
            dst: id2,
            kind: EdgeKind::Calls,
            meta: None,
        },
    )
    .unwrap();

    let edges = gocx::store::edges::get_edges_from(&store.conn, id1, &EdgeKind::Calls).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].dst, id2);

    // Resolution tracking
    assert!(!is_resolved(&store.conn, id1, &EdgeKind::Calls).unwrap());
    mark_resolved(&store.conn, id1, &EdgeKind::Calls, Some("v0.15.0")).unwrap();
    assert!(is_resolved(&store.conn, id1, &EdgeKind::Calls).unwrap());
}

#[test]
fn test_semantic_fixtures_indexable() {
    let fixtures = fixture_path().join("semantic");
    let tmp = TempDir::new().unwrap();
    let gocx_dir = tmp.path().join(".gocx");
    std::fs::create_dir_all(&gocx_dir).unwrap();

    // Copy semantic fixtures
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().map(|e| e == "go").unwrap_or(false) {
            std::fs::copy(entry.path(), tmp.path().join(entry.file_name())).unwrap();
        }
    }

    let db_path = gocx_dir.join("index.db");
    let mut store = gocx::store::Store::open_or_create(&db_path).unwrap();
    let stats = gocx::index::index_full(tmp.path(), &mut store.conn, false).unwrap();

    assert!(stats.files_parsed >= 2, "should parse handler.go and service.go");
    assert!(stats.symbols_extracted >= 8, "should extract all handler+service symbols");

    // Verify key symbols are present
    for name in &["UserHandler", "UserService", "DefaultUserService", "UserRepo"] {
        let q = gocx::store::symbols::FindQuery {
            query: name,
            exact: true,
            kind: None,
            package: None,
            limit: 1,
        };
        let results = gocx::store::symbols::find_symbols(&store.conn, &q).unwrap();
        assert!(!results.is_empty(), "should find symbol {}", name);
    }
}
