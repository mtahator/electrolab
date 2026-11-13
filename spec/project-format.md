# Library and project folder — format 1

Decision: [ADR-010](../docs/adr/ADR-010-project-folder-format.md). Owner: Workspace engine.

```
<folder>/
├─ electropaper-project.toml   manifest (below)
├─ workspace.sqlite            Workspace database (+ -wal, -shm while open)
├─ blobs/
│  ├─ tmp/                     partial writes; emptied on open
│  └─ ab/cd/abcd…(64 hex)      managed copies, named by BLAKE3 hash
└─ events/
   └─ seg-000001.jsonl         append-only, hash-chained event log
```

## Manifest

```toml
# ElectroPaper library/project folder. Do not edit by hand.
format = 1
id = "0190f5a2-…"           # UUID, stable across moves
kind = "library"            # library | project
name = "My Library"
created_ms = 1763000000000  # ms since the Unix epoch
```

## Database (`workspace.sqlite`, migration 1)

| Table `paper` | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `hash` | TEXT UNIQUE | BLAKE3 hex of the managed copy |
| `size`, `media_type` | INTEGER, TEXT | |
| `original_name`, `original_path` | TEXT | where it came from (information only) |
| `title`, `authors` (JSON array), `doi`, `pages`, `metadata_source` | | filled by `workspace.set_paper_metadata` |
| `added_ms` | INTEGER | |

### Migration 2 (library) and 3 (source address)

| Table | Columns |
|---|---|
| `paper` (added) | `opened_ms` (2), `source_url` (3) |
| `collection` | `id`, `name` (unique ignoring case), `created_ms` |
| `collection_paper` | `collection`, `paper`, `added_ms` (deleted with either side) |
| `paper_tag` | `paper`, `tag` |
| `reading_state` | `paper`, `location` (JSON `Location`), `zoom` (JSON `ZoomSpec`), `view_mode`, `updated_ms` |
| `bookmark` | `id`, `paper`, `name`, `location` (JSON), `created_ms` |
| `reading_time` | `paper`, `page`, `seconds`, `last_ms` |

## Event record (one JSON line)

```json
{"seq":1,"prev":"000…","hash":"…","event":{"at_ms":0,"actor":{"kind":"user"},"kind":"workspace.paper_added","data":{}}}
```

`hash = BLAKE3(prev ‖ seq (u64 little-endian) ‖ JSON(event))`. A torn last line is cut off on open; any other break is reported.

## Compatibility
- Readers accept `format` 1..=current and refuse newer ones without writing anything.
- Unknown files and folders are left alone (later versions add their own).
