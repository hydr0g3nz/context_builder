# `impact` — Blast-Radius Analysis

## ใช้ทำอะไร

`gocx impact <symbol>` วิเคราะห์ว่าถ้าเราแก้ไข symbol ตัวนึง จะกระทบโค้ดส่วนไหนบ้าง  
เหมาะสำหรับ AI หรือ developer ที่อยากรู้ **"ก่อนแก้ไฟล์นี้ ควรระวังอะไร?"**

---

## Pipeline การทำงาน

```
resolve_symbol(name)
       │
       ▼
GoplsClient::new()          ← spawn gopls subprocess
       │
       ▼
call_graph::callers()       ← BFS ย้อนขึ้นไปหา callers ทั้งหมด (depth=3 default)
       │
       ├─ direct_callers    (depth == 1)
       ├─ breakable_tests   (file ends with _test.go)
       └─ transitive_reach  (all_callers.len())
       │
       ▼
compute_risk_signals()      ← heuristic string matching, ไม่ต้องใช้ deps เพิ่ม
       │
       ▼
next_actions()              ← สร้าง hint commands สำหรับขั้นตอนถัดไป
       │
       ▼
ResponseEnvelope (JSON/Text output)
```

---

## โครงสร้างไฟล์

| ไฟล์ | หน้าที่ |
|------|---------|
| [src/impact/mod.rs](../src/impact/mod.rs) | logic หลักทั้งหมด: `run()`, `compute_risk_signals()`, `next_actions()` |
| [src/cli/impact.rs](../src/cli/impact.rs) | CLI entry point: parse args, spawn tokio runtime, emit output |

---

## Output — `ImpactReport`

```rust
pub struct ImpactReport {
    pub symbol: Symbol,               // symbol ที่ถูก query
    pub direct_callers: Vec<CallNode>, // callers ที่ depth=1
    pub transitive_reach: usize,       // จำนวน callers ทั้งหมด (BFS ทุก depth)
    pub risk_signals: Vec<String>,     // warning strings
    pub breakable_tests: Vec<CallNode>,// test files ที่จะพัง
}
```

JSON envelope ครอบอีกชั้น:
```json
{
  "query": "impact Save",
  "results": { ...ImpactReport... },
  "token_estimate": 142,
  "next_actions": ["gocx trace HandleSave Save", "gocx find-iface UserService"]
}
```

---

## Risk Signals (heuristic)

| เงื่อนไข | Signal ที่แสดง |
|----------|---------------|
| caller name มี `servehttp` / `handler` / `handlefunc` หรือ file มี `handler` | `"called from HTTP handler (HandlerName)"` |
| `transitive_reach > 10` | `"high fan-in (N callers)"` |
| มี callers ที่ไฟล์ลงท้าย `_test.go` | `"breaks N test(s) if changed"` |

ทั้งหมดเป็น string matching ล้วน ไม่มี dependency พิเศษ

---

## Next Actions (auto-generated hints)

| เงื่อนไข | Hint ที่สร้าง |
|---------|--------------|
| พบ HTTP handler ใน direct_callers | `gocx trace <handler> <sym>` |
| symbol เป็น `SymbolKind::Method` | `gocx find-iface <ReceiverType>` |

---

## BFS call graph (`call_graph::callers`)

- ใช้ **BFS** ผ่าน `petgraph` + `VecDeque`
- แต่ละ node: ถ้า edges ยังไม่ถูก resolve → spawn gopls ด้วย LSP `callHierarchy`
- edges ที่ resolve แล้วถูก cache ใน SQLite table `edge_resolution` → ครั้งต่อไปไม่ต้อง spawn gopls ใหม่
- convert LSP position (0-indexed) → DB lookup (1-indexed) ด้วย `+1` ทั้ง line และ character

---

## Graceful Degradation

ถ้า gopls ไม่ available (ไม่ติดตั้ง, ไม่มี go.mod, ฯลฯ):
- emit `ImpactReport` เปล่า (ไม่ crash)
- log warning ไปที่ stderr

---

## ตัวอย่างการใช้งาน

```bash
# วิเคราะห์ impact ของ method Save ลึก 3 ชั้น (default)
gocx impact Save

# ระบุ receiver ด้วยเพื่อความแม่นยำ
gocx impact UserService.Save

# เพิ่ม depth และ output เป็น text
gocx impact UserService.Save --depth 5 --output text
```

---

## ความสัมพันธ์กับ Phase อื่น

- **Phase 2** (`callers`, `callees`, `trace`) — `impact` ใช้ `call_graph::callers` เหมือนกัน แต่เพิ่ม risk analysis และ test detection
- **Phase 3** (`context`) — ใช้ cached edges จาก Phase 2 เหมือนกัน แต่ `impact` ยังต้อง spawn gopls สำหรับ edge ที่ยังไม่ถูก cache
