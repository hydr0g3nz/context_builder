# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`gocx` is an AI-first Go codebase intelligence CLI. It pre-indexes Go repositories into a SQLite symbol graph and exposes compact, token-efficient queries. Instead of grepping files and reading hundreds of lines, an AI can query the index in <50ms with structured JSON responses.

## Commands

```bash
make build          # cargo build (debug)
make release        # cargo build --release
make test           # cargo test
make test-verbose   # cargo test -- --nocapture
make lint           # cargo clippy -- -D warnings
make fmt            # rustfmt
make fmt-check      # check formatting without modifying
make check          # type-check only (no binary)
make install        # cargo install --path .
```

Run a single test:
```bash
cargo test test_index_fixtures
cargo test test_schema_v2_migration
```

## Architecture

Two execution phases:

**Phase 1 — Static index** (no gopls required): `init`, `index`, `status`, `find`, `pkg-tree`
**Phase 2 — Semantic queries** (spawns gopls): `callers`, `callees`, `trace`, `find-impls`, `find-iface`, `refs`

### Module Map

| Path | Role |
|------|------|
| `src/model.rs` | Core types: `Symbol`, `SymbolKind`, `Visibility`, `FileRecord` |
| `src/output.rs` | `ResponseEnvelope` — every command wraps output in `{query, results, token_estimate, next_actions}` |
| `src/cli/` | One file per subcommand; `mod.rs` owns the `Cli`/`Commands` enum |
| `src/store/` | SQLite layer — schema/migrations, symbol CRUD, edge CRUD, file metadata |
| `src/index/` | tree-sitter Go parser → symbols; rayon parallel file walking; hash-based incremental re-index |
| `src/gopls/` | Async tokio child process, JSON-RPC 2.0 with Content-Length framing, LSP queries |
| `src/semantic/` | BFS call graph traversal (petgraph), interface implementation resolution |

### Key Data Flows

**Indexing:** `.gitignore`-aware walker → tree-sitter AST → symbol extraction → rayon parallel batch → SQLite (WAL mode, transactions). Content hash + mtime per file drives incremental re-index.

**Semantic query (e.g., `callers`):** look up symbol in index → check `edge_resolution` cache → if cached, BFS via petgraph over cached edges → else spawn gopls, run LSP `callHierarchy`, resolve URI+line:col back to symbol IDs, cache edges, BFS.

**LSP transport:** tokio async child process (gopls), `RpcTransport` handles Content-Length framing, drains server notifications until "Finished loading packages" sentinel.

### Database Schema (SQLite v2, WAL)

- `files` — path, content hash, mtime for incremental indexing
- `symbols` — id, kind, name, package, file, line/col, signature, visibility; indexed for LIKE/NOCASE name search
- `edges` — src\_id, dst\_id, kind (`CALLS`, `IMPLEMENTS`, `USES_TYPE`, `EMBEDS`, `REFERENCES`)
- `edge_resolution` — tracks which symbols have had their edges resolved via gopls (cache invalidation by gopls version)

### Output Envelope

Every command returns:
```json
{ "query": "...", "results": [...], "truncated": false, "token_estimate": 142, "next_actions": [] }
```
Token estimate is `len(json) / 4`.

## Testing

Integration tests live in `tests/integration.rs` and use fixtures in `tests/fixtures/`:
- `simple.go` — basic User/UserService/UserStore definitions
- `semantic/` — multi-package handler+service example with a `go.mod`

Snapshot tests use `insta`; run `cargo insta review` to approve new snapshots.

## Known Invariants & Bug Patterns

### LSP position indexing
LSP (gopls) uses **0-indexed** line and character. The DB stores **1-indexed** positions (as written by tree-sitter extractor: `start.row + 1`, `start.column + 1`).

Every place that converts a gopls location to a DB lookup **must** apply `+ 1` to both line and character:
```rust
let line = loc.range.start.line as usize + 1;
let col  = loc.range.start.character as usize + 1;  // easy to forget
```
Affected files: `src/semantic/call_graph.rs` (callers + callees), `src/semantic/impls.rs`, `src/cli/refs.rs`.

### IMPLEMENTS edge direction
All IMPLEMENTS edges are stored as `src = interface_id → dst = concrete_id`.

- `find_implementations(iface)` → `get_edges_from(iface_id)` — src = iface ✓
- `find_interfaces_for(concrete)` → `get_edges_to(concrete_id)` — dst = concrete ✓

`resolve_impls` must flip the edge when queried with a concrete type (gopls returns interface locations in that case):
```rust
let (src, dst) = if sym_is_interface { (sym_id, impl_id) } else { (impl_id, sym_id) };
```

### Symbol name convention
Methods are stored as `ReceiverType.MethodName` (e.g., `S3Uploader.Upload`), never as bare `Upload`.
`resolve_symbol` prefers: exact name match → method-suffix match (`.{query}`) → first fuzzy result.
