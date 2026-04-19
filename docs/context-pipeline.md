# Context Pipeline (`src/context/`)

## จุดประสงค์

`gocx context <task>` รับข้อความ task ภาษาธรรมชาติ แล้วคืน **ชุด symbols ที่เกี่ยวข้องมากที่สุด** โดยไม่ต้องอ่านทุกไฟล์หรือ spawn gopls — ทำงานจาก SQLite graph ที่ cache ไว้ ทำให้เร็วมาก (<50ms)

ใช้สำหรับให้ AI รู้ว่า "ถ้าจะแก้ task นี้ ควรไปดูไฟล์/ฟังก์ชันไหนก่อน"

---

## Pipeline ภาพรวม

```
task text (free-text)
       │
       ▼
┌─────────────┐
│  seed.rs    │  ดึง Go identifier candidates จาก text → lookup ใน SQLite
└──────┬──────┘
       │  Vec<Symbol>  (seeds)
       ▼
┌─────────────┐
│  expand.rs  │  BFS depth-1 บน CALLS + IMPLEMENTS edges (bi-directional)
└──────┬──────┘
       │  Vec<ExpandedNode>  (seeds + neighbors)
       ▼
┌─────────────┐
│  rank.rs    │  score = 0.6 × distance_score + 0.4 × centrality_score
└──────┬──────┘
       │  Vec<RankedNode>  (top-N)
       ▼
┌─────────────┐
│  pack.rs    │  จัดกลุ่มตาม file path → FileGroup[] + summary string
└─────────────┘
       │
       ▼
ContextBundle { seeds, files, summary }
```

---

## แต่ละขั้นตอน

### 1. Seed Extraction (`seed.rs`)

**Input:** string ข้อความ task  
**Output:** `Vec<Symbol>` — symbols ที่ match กับ identifier ใน task

**วิธีทำงาน:**

1. แยก token จาก text (split ด้วย whitespace / comma / semicolon / quote)
2. **Pass 1 — CamelCase:** ถ้า token ขึ้นต้นด้วย uppercase → เป็น Go exported identifier → เพิ่มใน candidates  
   - ถ้ามี dot เช่น `UserService.Save` → เพิ่มทั้ง `UserService.Save` และ `UserService` แยกกัน
3. **Pass 2 — fuzzy fallback:** lowercase word ที่ยาว ≥ 4 ตัวอักษร (เช่น `save`, `upload`) → เพิ่มเป็น fuzzy candidate
4. Deduplicate แล้ว query `find_symbols()` ต่อ candidate (ใช้ `limit_per_candidate=3`)
5. Deduplicate ผลลัพธ์ด้วย `symbol.id`

```
task: "fix UserService.Save and upload flow"
candidates: ["UserService.Save", "UserService", "upload", "flow"]
→ seeds: [Symbol(UserService.Save), Symbol(UserService), Symbol(S3Uploader.Upload), ...]
```

---

### 2. Graph Expansion (`expand.rs`)

**Input:** `Vec<Symbol>` (seeds), `cap: usize`  
**Output:** `Vec<ExpandedNode>` — seeds + all depth-1 neighbors

**วิธีทำงาน:**

1. Insert seeds ทั้งหมดด้วย `distance = 0`
2. สำหรับแต่ละ seed ทำ BFS depth-1 บน 2 edge kinds:
   - `EdgeKind::Calls` — ฟังก์ชัน A เรียก B
   - `EdgeKind::Implements` — interface I มี concrete C
3. ดึงทั้ง **outgoing** (`get_edges_from`) และ **incoming** (`get_edges_to`) edges
4. แต่ละ neighbor ที่พบ → `distance = 1`, เพิ่ม `seed_references += 1` ทุกครั้งที่ seed อื่นอ้างถึง
5. หยุดเมื่อ node count ถึง `cap` (= `limit × 3`, min 30)
6. "Stub nodes" (มีแค่ id ยังไม่มีชื่อ) → resolve จาก DB ด้วย `find_symbol_by_id()`

**ข้อสังเกต:** ทำงานได้แม้ไม่มี gopls เพราะใช้แค่ edge cache ใน SQLite

---

### 3. Ranking (`rank.rs`)

**Input:** `Vec<ExpandedNode>`  
**Output:** `Vec<RankedNode>` — เรียง score จากมากไปน้อย, truncate ที่ `limit`

**สูตรคะแนน:**

```
distance_score   = 1.0 / (1.0 + distance)
                   → seed เอง (distance=0) = 1.0
                   → neighbor  (distance=1) = 0.5

centrality_score = seed_references / max_seed_references
                   → normalize 0.0–1.0

score = 0.6 × distance_score + 0.4 × centrality_score
```

Seeds ที่ตรง query (distance=0) ได้คะแนนสูงสุด  
Neighbor ที่ถูกอ้างถึงจากหลาย seeds ยิ่งได้คะแนนสูง

---

### 4. Packing (`pack.rs`)

**Input:** `Vec<RankedNode>`  
**Output:** `Vec<FileGroup>`, `summary: String`

**วิธีทำงาน:**

1. จัดกลุ่ม symbols ตาม `sym.file` (path)
2. เรียง symbols ใน group ด้วย score (มากไปน้อย)
3. เรียง FileGroups ด้วย top-symbol score ของแต่ละ group
4. สร้าง `summary` string เช่น:
   ```
   Task touches 2 package(s), 8 symbols seeding from UserService, Save; focus on internal/service/user.go
   ```

**Output struct:**
```rust
FileGroup {
    path: "internal/service/user.go",
    symbols: [
        PackedSymbol { name, kind, package, line, signature, doc, score },
        ...
    ]
}
```

---

### 5. Next Actions (`mod.rs`)

หลัง build bundle แล้ว `next_actions()` แนะนำ command ถัดไป:
- ถ้า top symbol เป็น `func` หรือ `method` → แนะนำ `gocx callers <name>` และ `gocx impact <name>`

---

## ข้อจำกัด / พฤติกรรมที่ควรรู้

| เรื่อง | รายละเอียด |
|--------|-----------|
| **BFS depth** | หยุดที่ depth-1 เท่านั้น ไม่ recursive — ถ้าต้องการ deep trace ใช้ `gocx trace` แทน |
| **Edge types** | ใช้แค่ `CALLS` และ `IMPLEMENTS` — ไม่รวม `USES_TYPE`, `EMBEDS`, `REFERENCES` |
| **Cache dependency** | ถ้ายังไม่เคยรัน Phase 2 command (`callers`/`callees` ฯลฯ) edges อาจยังว่างอยู่ → seeds จะได้แต่ distance=0 nodes |
| **Stub resolution** | neighbor nodes ถูก create เป็น stub ก่อน (มีแค่ id) แล้วค่อย resolve — ถ้า symbol ถูกลบออกจาก index แล้ว จะถูก skip |
| **cap = limit × 3** | expand ได้สูงสุด `limit*3` nodes ก่อน rank แล้ว truncate เหลือ `limit` |

---

## ตัวอย่างการใช้งาน

```bash
gocx context "fix the upload retry logic in S3Uploader"
gocx context "add rate limiting to HTTP handlers" --limit 20
```

Output JSON:
```json
{
  "query": "fix the upload retry logic in S3Uploader",
  "results": {
    "seeds": ["S3Uploader.Upload", "S3Uploader"],
    "files": [
      {
        "path": "internal/storage/s3.go",
        "symbols": [
          { "name": "S3Uploader.Upload", "kind": "method", "line": 42, "score": 1.0 }
        ]
      }
    ],
    "summary": "Task touches 1 package(s), 3 symbols seeding from S3Uploader.Upload; focus on internal/storage/s3.go"
  },
  "token_estimate": 87,
  "next_actions": ["gocx callers S3Uploader.Upload", "gocx impact S3Uploader.Upload"]
}
```
