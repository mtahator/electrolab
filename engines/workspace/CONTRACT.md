# Workspace engine — contract

Version: 0.2.0      Stability: experimental      Tier: T1
Crates: `ep-workspace-contract` (types), `ep-workspace-core` (format, classification, schema), `ep-workspace` (facade)

## Purpose
Libraries and projects: the folders that hold the user's papers, their managed copies and the record of what happened (ADR-010).

## Commands
All experimental. Errors are `CommandError::Failed { kind }` with the kinds below.

| Command | Input → Output | Permission |
|---|---|---|
| `workspace.create_project` | `{ folder, name, kind: library \| project }` → `ProjectInfo` (and opens it) | create |
| `workspace.open_project` | `{ folder }` → `ProjectInfo` | read |
| `workspace.open_default_library` | `null` → `ProjectInfo` (created on first run in the app data folder) | create |
| `workspace.current` | `null` → `ProjectInfo` or `null` | read |
| `workspace.classify` | `{ path }` → `{ kind: pdf \| unsupported, size }` (by content) | read |
| `workspace.add_file` | `{ path, source_url? }` → `{ paper: Paper, already_present }` | create |
| `workspace.get_paper` | `{ id }` → `Paper` | read |
| `workspace.list_papers` | `{ limit?, offset, collection?, tag?, query?, sort }` → `[Paper]` | read |
| `workspace.set_paper_metadata` | `{ id, title?, authors?, doi?, pages?, source }` → `Paper` | create |
| `workspace.create_collection` / `rename_collection` | `{ name }` / `{ id, name }` → `Collection` | create |
| `workspace.delete_collection` | `{ id }` → `null` (papers stay in the library) | **destructive** (the policy asks first) |
| `workspace.list_collections` | `null` → `[Collection { id, name, papers, created_ms }]` | read |
| `workspace.add_to_collection` / `remove_from_collection` | `{ collection, papers }` → `Collection` | create |
| `workspace.set_tags` | `{ paper, tags }` → `Paper` (tags trimmed, unique ignoring case) | create |
| `workspace.list_tags` | `null` → `[TagCount { tag, papers }]` | read |
| `workspace.get_reading_state` | `{ paper }` → `ReadingState { location?, zoom?, view_mode, bookmarks, opened_ms? }` | read |
| `workspace.save_reading_state` | `{ paper, location?, zoom?, view_mode }` → `null` (also marks the paper opened now) | create |
| `workspace.add_bookmark` / `remove_bookmark` | `{ name, location }` → `Bookmark` / `{ id }` → `null` | create |
| `workspace.record_reading` | `{ paper, page, seconds }` → `ReadingStats` (at most 300 s counted per report) | create |
| `workspace.reading_stats` | `{ paper }` → `ReadingStats { seconds, pages_read, last_ms? }` | read |

`workspace.list_papers` takes `{ limit?, offset, collection?, tag?, query?, sort: added | opened | title }`; `query` words match title, authors, DOI or file name. `workspace.add_file` takes an optional `source_url` (http/https/mailto only) for files downloaded from the web.

`ProjectInfo { id, name, kind, folder, format, created_ms, papers }`
`Paper { id, file: BlobRef, original_name, original_path?, source_url?, title?, authors, doi?, pages?, metadata_source?, tags, added_ms, opened_ms? }`

Error kinds: `no_project_open`, `not_found`, `unsupported_file`, `folder_in_use`, `bad_format`, `exists` (collection name taken), `io`.

## Events (hub)
- `workspace.opened` (`ProjectInfo`), `workspace.paper_added` (`Paper`), `workspace.paper_changed` (`Paper`).

The same facts are appended to the project's event log; opening a project also makes its log the bus's provenance log.

## Uses (contracts only)
- none (Workspace is a base engine, Draft 2.2 §6.3). Metadata comes from `document.metadata`, called by the host.

## Guarantees
1. The original file is never modified (tested: hash before = after).
2. The same content is stored once; adding it again returns the existing paper with `already_present = true`.
3. A library created, closed and reopened is identical (same `ProjectInfo`, same papers).
4. A folder written by a newer app is refused with `bad_format` and left unchanged.
5. Files are classified by content, never by name.

## Quality
- Conformance suite: `engines/workspace/facade/tests/conformance.rs` (through the bus).

## Changelog
- 0.2.0 (2026-11-13): collections, tags, library search and sort, reading state, bookmarks, reading time, source address (P1.M2.WSP.1–3, SH.7); migrations 2–3.
- 0.1.0 (2026-11-13): first slice (P1.M1.WSP.1–3).
