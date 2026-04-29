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
cargo test test_schema_v2_migration   # now verifies v3 + line_end column
cargo test --lib flow                 # flow module unit tests (controlflow, render)
```

## Architecture

Three execution phases:

**Phase 1 — Static index** (no gopls required): `init`, `index`, `status`, `find`, `pkg-tree`
**Phase 2 — Semantic queries** (spawns gopls): `callers`, `callees`, `trace`, `find-impls`, `find-iface`, `refs`
**Phase 3 — AI-native** (uses cached graph, gopls optional): `impact`, `context`
**Phase 3+ — Flow navigator** (requires gopls): `flow`

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
| `src/impact/` | Blast-radius analysis: caller BFS + risk signals + breakable test detection |
| `src/context/` | Smart context: seed extraction → graph expand → rank → pack by file |
| `src/flow/` | AI-readable call+control-flow tree: `controlflow.rs` (tree-sitter CF extractor), `tree.rs` (DFS builder), `render.rs` (indented text), `interface.rs` (eager impl expansion) |

### Key Data Flows

**Indexing:** `.gitignore`-aware walker → tree-sitter AST → symbol extraction → rayon parallel batch → SQLite (WAL mode, transactions). Content hash + mtime per file drives incremental re-index.

**Semantic query (e.g., `callers`):** look up symbol in index → check `edge_resolution` cache → if cached, BFS via petgraph over cached edges → else spawn gopls, run LSP `callHierarchy`, resolve URI+line:col back to symbol IDs, cache edges, BFS.

**LSP transport:** tokio async child process (gopls), `RpcTransport` handles Content-Length framing, drains server notifications until "Finished loading packages" sentinel.

**Impact analysis (`impact`):** resolve symbol → BFS callers via `semantic::call_graph::callers` → partition into direct/test/transitive → apply heuristic risk signals (HTTP handler pattern, high fan-in, test breakage) → emit `next_actions`.

**Smart context (`context`):** extract Go identifier seeds from free-text task → BFS depth-1 over cached CALLS+IMPLEMENTS edges (no gopls spawn needed) → score by distance+centrality → group by file path. Works offline from cached graph.

### Database Schema (SQLite v3, WAL)

- `files` — path, content hash, mtime for incremental indexing
- `symbols` — id, kind, name, package, file, line/col, **line_end** (end line of declaration body), signature, visibility; indexed for LIKE/NOCASE name search
- `edges` — src\_id, dst\_id, kind (`CALLS`, `IMPLEMENTS`, `USES_TYPE`, `EMBEDS`, `REFERENCES`)
- `edge_resolution` — tracks which symbols have had their edges resolved via gopls (cache invalidation by gopls version)

`line_end` is populated by the tree-sitter extractor from the declaration node's end position (Func/Method only; NULL for Struct/Interface/TypeAlias). Used by `gocx flow` to scope control-flow extraction. Populated on re-index; NULL for rows indexed before v3.

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

## Phase 3 — Impact, Context & Flow

### `gocx impact <sym> [--depth=3]`
- Spawns gopls; runs `semantic::call_graph::callers` BFS
- Partitions results: `direct_callers` (depth=1), `breakable_tests` (file ends `_test.go`), `transitive_reach` (total count)
- Risk signals (string heuristics, no extra deps):
  - `"called from HTTP handler"` — caller name contains `servehttp`/`handler`/`handlefunc` or file contains `handler`
  - `"high fan-in (N callers)"` — transitive_reach > 10
  - `"breaks N test(s) if changed"` — breakable_tests non-empty
- `next_actions`: `gocx trace <handler> <sym>` if HTTP handler found; `gocx find-iface <ReceiverType>` if sym is a method

### `gocx context <task> [--limit=30]`
Pipeline in `src/context/`:
1. **seed.rs** — regex-extract CamelCase tokens + words ≥4 chars from task text → `find_symbols` lookup → deduplicated seed list
2. **expand.rs** — BFS depth-1 from each seed over cached `CALLS` + `IMPLEMENTS` edges (both directions); cap at `limit*3` nodes
3. **rank.rs** — score = `0.6 * distance_score + 0.4 * centrality_score`; sort descending; truncate to `limit`
4. **pack.rs** — group ranked symbols by file path; build summary string
- Works fully offline if Phase 2 edges are cached; logs debug warning when edges are missing for a seed

### `gocx flow <root> [--depth=3] [--exclude=<substr>] [--json]`

Pipeline in `src/flow/`:
1. **interface.rs** — `resolve_interface_impls`: if callee is `Interface` or a method on an interface, call `find_implementations` (eager, cached)
2. **controlflow.rs** — `ControlFlowExtractor::extract_in_range`: tree-sitter query for `if_statement`, `expression_switch_statement`, `type_switch_statement`, `select_statement`, `go_statement`, `defer_statement`, `communication_case`; filtered to `[sym.line, sym.line_end]`
3. **tree.rs** — `build_flow`: DFS from root; for each function: resolve callees via `resolve_and_cache_callees`, extract CF nodes, merge both by line order; eager-expand `[INTF]` to `[IMPL]`; cycle detection via visited set
4. **render.rs** — `render_text`: box-drawing indented tree; `--json` emits `ResponseEnvelope` with `FlowNode` tree

Output notation: `[ROOT]`, `[CALL]`, `[INTF]`, `[IMPL]`, `[IF]`, `[SWITCH]`, `[SELECT]`, `[GO]`, `[DEFER]`, `[CASE]` — every node has `file:line:col label signature?`

Node truncation reasons: `"cycle"` (revisited symbol), `"max_depth"` (depth limit hit).

`for`/`range` loops intentionally excluded from L2 — reduces noise for large loops.

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

### `SymbolKind` / `EdgeKind` parsing
Both types have a `parse(s: &str) -> Option<Self>` method (not `from_str` — avoids clippy `should_implement_trait` lint). Call sites: `store/symbols.rs` (`SymbolKind::parse`), `store/edges.rs` (`EdgeKind::parse`).
