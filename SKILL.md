---
name: gocx
description: AI-first Go codebase intelligence CLI. Use this skill when working with Go repositories that have been indexed with gocx. Provides fast symbol lookup, call graph traversal, blast-radius analysis, and smart context generation — replacing slow grep/cat patterns with <50ms structured JSON queries.
triggers:
  - "find symbol in go repo"
  - "what calls this function"
  - "impact of changing"
  - "context for task in go codebase"
  - "who implements this interface"
  - "trace call path"
  - "gocx"
---

# gocx — Go Codebase Intelligence

`gocx` pre-indexes Go repositories into a SQLite symbol graph and exposes token-efficient queries. Use it instead of grep+cat to reduce token usage 10-100x when navigating Go code.

## Quick Start

```bash
gocx init .          # one-time: create .gocx/ index
gocx index .         # build symbol index (~1s/10k LOC)
gocx status .        # verify index health
```

## Command Reference

### Phase 1 — Static Index (always available)

```bash
gocx find <query>                    # search symbols by name (substring)
gocx find <query> --exact            # exact name match
gocx find <query> --kind func        # filter by kind: func|method|struct|interface|const|var
gocx pkg-tree .                      # show package structure (JSON or text)
gocx status .                        # index stats: symbol count, file count, last indexed
```

### Phase 2 — Semantic Queries (requires gopls in PATH)

```bash
gocx callers <symbol> [--depth 3]    # BFS find all callers (first call: ~2-8s; cached: <50ms)
gocx callees <symbol> [--depth 3]    # BFS find all callees
gocx trace <from> <to>               # shortest call path between two symbols
gocx find-impls <interface>          # concrete types implementing interface
gocx find-iface <type>               # interfaces satisfied by a concrete type
gocx refs <symbol>                   # all references to a symbol
```

### Phase 3 — AI-Native (offline after Phase 2 caching)

```bash
gocx impact <symbol> [--depth 3]     # blast-radius: callers + risk signals + next_actions
gocx context "<task>" [--limit 30]   # free-text → ranked symbol bundle for task
```

## Recommended Workflows

### Before Modifying a Symbol
```bash
gocx impact UserService.Save --depth 3
# Returns: direct_callers, breakable_tests, transitive_reach, risk_signals, next_actions
```

### Starting a New Task
```bash
gocx context "add JWT authentication to login flow" --limit 20
# Returns: ranked list of relevant symbols grouped by file — your reading list
```

### Navigating Unfamiliar Code
```bash
gocx find UserService                # locate the declaration
gocx callers UserService.Save        # who uses it?
gocx callees UserService.Save        # what does it call?
gocx find-impls Repository           # concrete implementations of interface
```

### Tracing a Request Path
```bash
gocx trace HandleLogin UserService.Save   # full call chain between two points
```

## Output Format

Every command returns a `ResponseEnvelope`:
```json
{
  "query": "impact UserService.Save",
  "results": { ... },
  "truncated": false,
  "token_estimate": 142,
  "next_actions": ["gocx trace HandleLogin UserService.Save"]
}
```

Always check `next_actions` — gocx suggests the most relevant follow-up queries automatically.

## Symbol Naming Conventions

- **Methods**: `ReceiverType.MethodName` — e.g., `UserService.Save`, `S3Uploader.Upload`
- **Functions**: bare name — e.g., `HandleLogin`, `parseConfig`
- **Interfaces**: bare name — e.g., `Repository`, `UserStore`

When in doubt, use `gocx find <partial>` to discover the exact name.

## Risk Signals (impact command)

| Signal | Meaning |
|--------|---------|
| `called from HTTP handler` | Change touches a public endpoint — high blast radius |
| `high fan-in (N callers)` | Many dependents — breaking change risk is elevated |
| `breaks N test(s) if changed` | Test files in call chain will need updating |

## Performance Expectations

| Operation | Time |
|-----------|------|
| `find`, `status`, `pkg-tree` | <10ms |
| Phase 2 (first call, gopls warm-up) | 2–8s |
| Phase 2 (subsequent, cached) | <50ms |
| `context` (offline, edges cached) | <50ms |
| Initial index (100k LOC) | ~10s |

## When Edges Are Not Cached

If Phase 2 commands (callers, callees, etc.) haven't been run yet, `context` will log a debug warning about missing edges. Run one Phase 2 query to warm the cache:

```bash
gocx callers <any-symbol>   # warms gopls + caches edges for BFS
```

## Integration with AI Workflows

**Prefer gocx over grep+cat when:**
- Looking up where a symbol is defined → `gocx find`
- Assessing risk of a change → `gocx impact`
- Building context for a task → `gocx context`
- Understanding call relationships → `gocx callers` / `gocx callees`

**Use grep/cat when:**
- Reading full function bodies (gocx shows metadata, not source)
- Searching string literals or comments
- Working in non-Go files
