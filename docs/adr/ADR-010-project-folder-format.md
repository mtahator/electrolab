# ADR-010: Library and project folder format v1

Status: **accepted**
Date: 2026-11-13
Plan reference: Draft 2.2 §9.4 (blobs), §9.5 (events), §12 (projects); tasks P1.M1.STO.6, P1.M1.WSP.1–2

## Context
A library or project must survive app updates, moves between folders and machines, sync tools and crashes. The user's original files must never change. Later phases add notebooks, experiments and sync, so the format needs a version and room to grow.

## Decision
1. **One folder per library or project**, identified by a manifest `electropaper-project.toml` (`format`, `id`, `kind`, `name`, `created_ms`). The id is stable when the folder moves.
2. **Managed copies:** every added PDF is copied into `blobs/` under its BLAKE3 hash (`blobs/ab/cd/<hash>`). The original is only read. The same content is stored once.
3. **Database:** `workspace.sqlite` (WAL), owned by the Workspace engine; other engines get their own database files in the same folder when they need one. Migrations are numbered; a database newer than the app is refused, never downgraded.
4. **Event log:** `events/seg-NNNNNN.jsonl`, append-only and hash-chained (provenance of every non-read command, and domain events).
5. **Paths inside the folder are relative and `/`-separated**; absolute paths appear only as "where it came from" information, never as a way to find data.
6. **Versioning:** the app opens formats `1..=current`; a newer `format` is refused with "update the app"; upgrades are explicit migrations with a backup.
7. **Disposable data** (render caches, search indexes that can be rebuilt) lives outside the folder, in the app cache, keyed by blob hash.

## Alternatives considered
| Option | Why not |
|---|---|
| One database for everything, no folder | Hard to back up, move or share one project; one corrupt file loses all |
| Keep PDFs where the user put them | Moves and renames break links; annotations would have to modify originals |
| A single archive file per project | Not friendly to sync tools and partial reads; an export can come later |

## Consequences
- Moving or copying a project folder keeps everything working.
- Disk use grows with managed copies (deduplicated by content).
- Annotation export writes a new file (or an incremental update of the managed copy, ADR-006), never the original.

## Evidence
`engines/workspace/facade/tests/conformance.rs` (create → add → close → reopen is identical; a newer format is refused and left unchanged); `spec/project-format.md`.
