# gocx — Go Codebase Intelligence CLI

> "LSP คือ database สำหรับ IDE; gocx คือ database สำหรับ AI"

CLI tool ที่ pre-index Go codebase เป็น semantic symbol graph แล้ว expose high-level queries ที่ AI (Claude Code) เรียกผ่าน bash ได้ — ได้คำตอบแม่น + ประหยัด token 10-100x เทียบกับ grep+read ปกติ

## Why gocx?

| | Vanilla Claude Code | gocx |
|---|---|---|
| Find symbol | grep ทั้ง repo (อ่านหลายไฟล์) | index query <50ms |
| Semantic search | ไม่มี (false positives) | symbol kind + package filter |
| Impact analysis | ไม่มี | Phase 2: call graph |
| Token usage | สูง (อ่าน full files) | ต่ำ (compact JSON) |
| Cross-file trace | ไม่ได้ | Phase 2: callers/callees |

## Install

```bash
cargo install --path .
# หรือ build ด้วยตัวเอง
cargo build --release
# binary อยู่ที่ target/release/gocx
```

## Quick Start

```bash
# 1. init ใน root ของ Go repo
gocx init /path/to/your-go-repo

# 2. build index (ครั้งแรก ~5-30s ขึ้นกับขนาด repo)
gocx index /path/to/your-go-repo

# 3. query!
gocx find UserService
gocx pkg-tree /path/to/your-go-repo --output text
```

## Commands

### `gocx init [path]`

สร้าง `.gocx/` index directory ใน Go repo

```bash
gocx init .
gocx init /path/to/repo
```

สร้างไฟล์:
- `.gocx/index.db` — SQLite database
- `.gocx/.gitignore` — ป้องกัน index leak เข้า git
- `.gocx/config.toml` — module name + root path

---

### `gocx index [path] [--full | --incremental] [--include-tests]`

Build หรือ update symbol index

```bash
gocx index .                    # full re-index (default)
gocx index . --full             # explicit full re-index
gocx index . --incremental      # re-parse เฉพาะไฟล์ที่เปลี่ยน
gocx index . --include-tests    # รวม _test.go files ด้วย
```

---

### `gocx status [path]`

แสดง health check + สถิติ index

```
gocx Status
  Index path:     /repo/.gocx/index.db
  Schema version: 1
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

### `gocx find <query> [options]`

ค้นหา symbol ด้วยชื่อ (substring match by default)

```bash
gocx find UserService
gocx find Save --kind method
gocx find --exact "UserService.Save"
gocx find Handler --package http --limit 10
gocx find Reader --output text
```

**Options:**
| Flag | Default | Description |
|---|---|---|
| `--exact` | false | exact name match (case-sensitive) |
| `--kind` | — | filter: `func`, `method`, `struct`, `interface`, `type_alias`, `const`, `var` |
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
    },
    {
      "name": "UserService.Save",
      "kind": "method",
      "package": "userservice",
      "file": "internal/user/service.go",
      "line": 30,
      "signature": "(ctx context.Context, u *User) error",
      "doc": "Save persists a user.",
      "visibility": "exported"
    }
  ],
  "truncated": false,
  "token_estimate": 142,
  "next_actions": []
}
```

---

### `gocx pkg-tree [path] [--output json|text]`

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

internal/user/repository (8)
  func         2
  method       4
  struct       1
  interface    1
```

---

## Output Format

ทุก command รองรับ `--output json` (default) และ `--output text`

JSON envelope มี fields:
- `query` — command ที่รัน
- `results` — ผลลัพธ์ (command-specific)
- `truncated` — true ถ้าผลถูกตัดเพราะเกิน `--limit`
- `token_estimate` — ประมาณ token count (`len(json) / 4`)
- `next_actions` — suggested follow-up queries (Phase 2+)

---

## How Claude Code Uses gocx

แทนที่จะ:
```
# Claude ทำแบบนี้ (แพง + ช้า)
grep -r "UserService" . | head -50   # อ่านหลายไฟล์
cat internal/user/service.go          # อ่านทั้งไฟล์ 300 บรรทัด
```

ใช้:
```bash
# Claude ทำแบบนี้ (เร็ว + ประหยัด token)
gocx find UserService
# → compact JSON, แค่ symbols ที่ต้องการ, ~150 tokens แทน ~3000
```

ตัวอย่าง prompt สำหรับ Claude:
```
You have gocx available. Before reading files, use:
- `gocx find <name>` to locate symbols
- `gocx pkg-tree .` to understand project structure
- `gocx status .` to see what's indexed
This saves tokens significantly.
```

---

## Storage

Index ถูก store ที่ `<repo>/.gocx/index.db` (SQLite, WAL mode)

`.gocx/.gitignore` มี `*` ไว้แล้ว — index จะไม่ถูก commit โดย default
ถ้าอยากแชร์ index ใน team ให้ลบบรรทัดนั้นออกจาก `.gitignore`

---

## Performance

| Repo size | Full index | find query |
|---|---|---|
| 10k LOC | <1s | <10ms |
| 100k LOC | <10s | <50ms |
| 1M LOC | <2min | <50ms |

Incremental index (`--incremental`) re-parse เฉพาะไฟล์ที่ content hash เปลี่ยน

---

## Roadmap

- **Phase 1** ✅ Symbol index: `init`, `index`, `status`, `find`, `pkg-tree`
- **Phase 2** 🔜 Semantic: `callers`, `callees`, `find-impls`, `trace` (via gopls)
- **Phase 3** 🔜 AI-native: `context <task>`, `impact <symbol>`, token budget, `next_actions`
- **Phase 4** 🔜 Scale: daemon mode, file watcher, incremental events, 1M LOC benchmark
- **Phase 5** 🔮 MCP server, multi-language, VSCode extension

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
│   └── pkg_tree.rs      # gocx pkg-tree
├── index/
│   ├── extractor.rs     # tree-sitter → Symbol records
│   ├── walker.rs        # .gitignore-aware Go file walker
│   └── mod.rs           # parallel indexer (rayon)
└── store/
    ├── schema.rs        # SQLite DDL + migrations
    ├── symbols.rs       # symbol CRUD + find query
    └── files.rs         # file metadata + hash tracking
```
