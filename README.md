# gocx — Go Codebase Intelligence CLI

> "LSP คือ database สำหรับ IDE; gocx คือ database สำหรับ AI"

CLI tool ที่ pre-index Go codebase เป็น semantic symbol graph แล้ว expose high-level queries ที่ AI (Claude Code) เรียกผ่าน bash ได้ — ได้คำตอบแม่น + ประหยัด token 10-100x เทียบกับ grep+read ปกติ

## Why gocx?

| | Vanilla Claude Code | gocx |
|---|---|---|
| Find symbol | grep ทั้ง repo (อ่านหลายไฟล์) | index query <50ms |
| Semantic search | ไม่มี (false positives) | symbol kind + package filter |
| Call graph | ไม่มี | `callers` / `callees` via gopls |
| Impact analysis | ไม่มี | Phase 3: `impact` command |
| Token usage | สูง (อ่าน full files) | ต่ำ (compact JSON) |
| Cross-file trace | ไม่ได้ | `trace <from> <to>` |

---

## Install

```bash
cargo install --path .
# หรือ build ด้วยตัวเอง
cargo build --release
# binary อยู่ที่ target/release/gocx
```

**ข้อกำหนดสำหรับ Phase 2 commands** (`callers`, `callees`, `trace`, `find-impls`, `find-iface`, `refs`):

```bash
go install golang.org/x/tools/gopls@latest
```

---

## Quick Start

```bash
# 1. init ใน root ของ Go repo
gocx init /path/to/your-go-repo

# 2. build index (ครั้งแรก ~5-30s ขึ้นกับขนาด repo)
gocx index /path/to/your-go-repo

# 3. query!
gocx find UserService
gocx pkg-tree . --output text

# Phase 2: semantic queries (ต้องมี gopls)
gocx callers Save
gocx find-impls UserService
gocx trace HandleCreate Save
```

---

## Commands

### Phase 1 — Symbol Index

#### `gocx init [path]`

สร้าง `.gocx/` index directory ใน Go repo

```bash
gocx init .
gocx init /path/to/repo
```

สร้างไฟล์:
- `.gocx/index.db` — SQLite database (schema v2)
- `.gocx/.gitignore` — ป้องกัน index leak เข้า git
- `.gocx/config.toml` — module name + root path

---

#### `gocx index [path] [--full | --incremental] [--include-tests]`

Build หรือ update symbol index

```bash
gocx index .                    # full re-index (default)
gocx index . --incremental      # re-parse เฉพาะไฟล์ที่เปลี่ยน
gocx index . --include-tests    # รวม _test.go files ด้วย
```

---

#### `gocx status [path]`

แสดง health check + สถิติ index

```
gocx Status
  Index path:     /repo/.gocx/index.db
  Schema version: 2
  Files indexed:  842
  Total symbols:  12,450
  DB size:        8.2 MB
  Last indexed:   2026-04-17 10:30:00 UTC

Symbols by kind:
  method       5,210
  func         3,100
  struct       2,400
  interface      890
  type_alias     850
```

---

#### `gocx find <query> [options]`

ค้นหา symbol ด้วยชื่อ (substring match by default)

```bash
gocx find UserService
gocx find Save --kind method
gocx find --exact "UserService.Save"
gocx find Handler --package http --limit 10
gocx find Reader --output text
```

| Flag | Default | Description |
|---|---|---|
| `--exact` | false | exact name match |
| `--kind` | — | `func`, `method`, `struct`, `interface`, `type_alias`, `const`, `var` |
| `--package` | — | filter by package name |
| `--limit N` | 20 | max results |
| `--output` | `json` | `json` หรือ `text` |

**JSON output:**
```json
{
  "query": "find UserService",
  "results": [
    {
      "name": "UserService",
      "kind": "struct",
      "package": "userservice",
      "file": "internal/user/service.go",
      "line": 14,
      "visibility": "exported"
    }
  ],
  "truncated": false,
  "token_estimate": 142,
  "next_actions": []
}
```

---

#### `gocx pkg-tree [path] [--output json|text]`

แสดง package structure + symbol counts

```bash
gocx pkg-tree .
gocx pkg-tree . --output text
```

**Text output:**
```
internal/user/service (12)
  func         3
  method       5
  struct       2
  interface    2
```

---

### Phase 2 — Semantic Queries (ต้องมี `gopls`)

คำสั่ง Phase 2 ทุกตัว spawn `gopls` ครั้งแรกที่เรียก, resolve edges ผ่าน JSON-RPC แล้ว cache ลง SQLite — ครั้งต่อไปที่ query เดิมจะตอบจาก cache ทันที

ถ้าไม่มี `gopls` ใน PATH จะแสดง warning และ return ผลที่มีใน cache (ถ้ามี)

---

#### `gocx callers <symbol> [--depth N] [--output json|text]`

หา caller ทั้งหมดของ symbol (BFS ไม่เกิน `--depth` hops)

```bash
gocx callers Save
gocx callers "UserService.Save" --depth 3
gocx callers HandleCreate --output text
```

---

#### `gocx callees <symbol> [--depth N] [--output json|text]`

หา callee ทั้งหมดของ symbol

```bash
gocx callees HandleCreate
gocx callees NewUserService --depth 2
```

---

#### `gocx trace <from> <to> [--max-depth N] [--output json|text]`

หา call path ที่สั้นที่สุดระหว่างสอง symbol

```bash
gocx trace HandleCreate Save
gocx trace ServeHTTP Insert --max-depth 10
```

**Text output:**
```
Call path (3 hops):
    HandleCreate  (handler/handler.go:26)
  → Save          (handler/service.go:22)
  → Insert        (handler/repo.go:15)
```

---

#### `gocx find-impls <interface> [--output json|text]`

หา concrete type ทั้งหมดที่ implement interface นั้น

```bash
gocx find-impls UserService
gocx find-impls UserRepo --output text
```

---

#### `gocx find-iface <type> [--output json|text]`

หา interface ทั้งหมดที่ concrete type นั้น satisfy

```bash
gocx find-iface DefaultUserService
gocx find-iface SqlUserRepo --output text
```

---

#### `gocx refs <symbol> [--output json|text]`

หา reference ทั้งหมดของ symbol ใน codebase

```bash
gocx refs UserService
gocx refs Save --output text
```

---

## Output Format

ทุก command รองรับ `--output json` (default) และ `--output text`

JSON envelope fields:
- `query` — command ที่รัน
- `results` — ผลลัพธ์ (command-specific)
- `truncated` — true ถ้าผลถูกตัดเพราะเกิน `--limit`
- `token_estimate` — ประมาณ token count (`len(json) / 4`)
- `next_actions` — suggested follow-up queries (Phase 3+)

---

## How Claude Code Uses gocx

แทนที่จะ:
```bash
# Claude ทำแบบนี้ (แพง + ช้า)
grep -r "UserService" . | head -50   # อ่านหลายไฟล์
cat internal/user/service.go          # อ่านทั้งไฟล์ 300 บรรทัด
```

ใช้:
```bash
# Claude ทำแบบนี้ (เร็ว + ประหยัด token)
gocx find UserService              # locate symbol → ~150 tokens
gocx callers Save --depth 2        # who calls Save? → compact JSON
gocx find-impls UserService        # implementations → no grep needed
```

ตัวอย่าง system prompt สำหรับ Claude:
```
You have gocx available. Before reading files, use:
- `gocx find <name>` to locate symbols
- `gocx callers <name>` to find callers (requires gopls)
- `gocx find-impls <iface>` to find implementations
- `gocx pkg-tree .` to understand project structure
This saves tokens significantly.
```

---

## Storage

Index ถูก store ที่ `<repo>/.gocx/index.db` (SQLite, WAL mode, schema v2)

| Table | เก็บอะไร |
|---|---|
| `symbols` | declarations ทุก symbol (kind, name, package, file, line…) |
| `files` | content hash + mtime ของทุกไฟล์ (ใช้ incremental) |
| `edges` | call graph edges: `CALLS`, `IMPLEMENTS`, `USES_TYPE`, … |
| `edge_resolution` | cache status ว่า symbol ไหน resolve ผ่าน gopls แล้ว |
| `meta` | schema version, last index timestamp |

`.gocx/.gitignore` มี `*` ไว้แล้ว — index จะไม่ถูก commit โดย default

---

## Performance

| Repo size | Full index | `find` query |
|---|---|---|
| 10k LOC | <1s | <10ms |
| 100k LOC | <10s | <50ms |
| 1M LOC | <2min | <50ms |

Incremental index (`--incremental`) re-parse เฉพาะไฟล์ที่ content hash เปลี่ยน

Semantic queries (`callers` etc.) ครั้งแรกมี overhead จาก gopls warm-up (~2-8s); ครั้งต่อไป serve จาก SQLite cache

---

## Roadmap

| Phase | Status | Features |
|---|---|---|
| 1 | ✅ Complete | `init`, `index`, `status`, `find`, `pkg-tree` |
| 2 | ✅ Complete | `callers`, `callees`, `trace`, `find-impls`, `find-iface`, `refs` + gopls client + edges table |
| 3 | 🔜 Planned | `context <task>`, `impact <symbol>`, token budget, `next_actions` chain hints |
| 4 | 🔜 Planned | daemon mode, file watcher, incremental events, 1M LOC benchmark |
| 5 | 🔮 Future | MCP server, TypeScript/Python/Rust backends, federated queries |

---

## Benchmark

```powershell
# ทดสอบ performance กับ kubernetes/client-go (~100k LOC)
.\scripts\bench.ps1
```

---

## Project Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # library exports
├── model.rs             # Symbol, SymbolKind, Visibility, FileRecord
├── output.rs            # JSON envelope formatter
├── cli/
│   ├── init.rs          # gocx init
│   ├── index.rs         # gocx index
│   ├── status.rs        # gocx status
│   ├── find.rs          # gocx find
│   ├── pkg_tree.rs      # gocx pkg-tree
│   ├── callers.rs       # gocx callers  (Phase 2)
│   ├── callees.rs       # gocx callees  (Phase 2)
│   ├── trace.rs         # gocx trace    (Phase 2)
│   ├── find_impls.rs    # gocx find-impls (Phase 2)
│   ├── find_iface.rs    # gocx find-iface (Phase 2)
│   ├── refs.rs          # gocx refs     (Phase 2)
│   └── resolve.rs       # shared symbol resolver
├── gopls/               # (Phase 2) gopls JSON-RPC client
│   ├── mod.rs           # GoplsClient lifecycle
│   ├── protocol.rs      # LSP request/response types
│   ├── rpc.rs           # Content-Length framing over tokio
│   └── queries.rs       # callers/callees/refs/impls queries
├── semantic/            # (Phase 2) graph algorithms
│   ├── call_graph.rs    # BFS callers/callees + trace_path (petgraph)
│   └── impls.rs         # interface implementation resolver
├── index/
│   ├── extractor.rs     # tree-sitter → Symbol records
│   ├── walker.rs        # .gitignore-aware Go file walker
│   └── mod.rs           # parallel indexer (rayon)
└── store/
    ├── schema.rs        # SQLite DDL + migrations (v1→v2)
    ├── symbols.rs       # symbol CRUD + find query
    ├── files.rs         # file metadata + hash tracking
    └── edges.rs         # (Phase 2) edge CRUD + resolution cache
```
