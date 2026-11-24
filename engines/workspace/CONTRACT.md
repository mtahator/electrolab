# Workspace engine — contract

Version: 0.6.0      Stability: experimental      Tier: T1
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
| `workspace.set_paper_file` | `{ id, file }` → `Paper` (a new version of the managed copy, already in the blob store, e.g. from `document.annotate`; the previous version is kept and listed in `paper_version`) | create |
| `workspace.get_notes` | `{ paper }` → `PaperNotes { paper, text, version, modified_ms? }` (empty, version 0, when nothing was written) | read |
| `workspace.save_notes` | `{ paper, text, base_version? }` → `PaperNotes` (a new version; refused with `conflict` when `base_version` is given and the notes moved on) | create |
| `workspace.notes_history` | `{ paper }` → `[NotesVersion { version, saved_ms, chars, preview }]`, newest first (one entry per minute of editing, at most 200) | read |
| `workspace.restore_notes` | `{ paper, version }` → `PaperNotes` (the old text as a new version) | create |
| `workspace.set_paper_metadata` | `{ id, title?, authors?, doi?, pages?, source }` → `Paper` (also used to rename: `title`, `source: "user"`) | create |
| `workspace.remove_paper` | `{ id }` → `null`: the paper leaves the library at once (lists no longer show it); it can be brought back with `restore_paper` until the library is next opened, when removed papers and their managed copies are deleted for good | create (undoable) |
| `workspace.restore_paper` | `{ id }` → `Paper`: undo of `remove_paper` | create |
| `workspace.save_digest` | `{ paper, data }` → `null`: kept as `digests/<paper id>.json` in the library folder, next to the managed copy (at most 4 MB; made and read by the Knowledge engine; goes when the paper is purged) | create |
| `workspace.get_digests` | `{ papers? }` → `[PaperDigest { paper, data }]` | read |
| `workspace.create_collection` / `rename_collection` | `{ name, parent? }` / `{ id, name }` → `Collection` (a folder; names are unique in one place) | create |
| `workspace.move_collection` | `{ id, parent? }` → `Collection` (never into itself or its own folders; at most 12 deep) | create |
| `workspace.apply_folders` | `{ groups: [{ papers, collection? \| new?: { name, parent? } }] }` → `AppliedFolders { created, before }`: each group goes into an existing or a new folder, or out of all folders; **all of it or nothing**; a paper is then in that folder only | create (undoable) |
| `workspace.undo_folders` | `AppliedFolders` → `null`: the papers go back, folders made for it go when nothing else is in them | create |
| `workspace.remove_papers` / `restore_papers` | `{ ids }` → `null`: several at once, as `remove_paper` / `restore_paper` | create (undoable) |
| `workspace.delete_collection` | `{ id }` → `null` (the folders in it go too; papers stay in the library) | **destructive** (the policy asks first) |
| `workspace.list_collections` | `null` → `[Collection { id, name, parent?, papers, created_ms }]` | read |
| `workspace.add_to_collection` / `remove_from_collection` | `{ collection, papers }` → `Collection` | create |
| `workspace.set_tags` | `{ paper, tags }` → `Paper` (tags trimmed, unique ignoring case) | create |
| `workspace.list_tags` | `null` → `[TagCount { tag, papers }]` | read |
| `workspace.get_reading_state` | `{ paper }` → `ReadingState { location?, zoom?, view_mode, bookmarks, opened_ms? }` | read |
| `workspace.save_reading_state` | `{ paper, location?, zoom?, view_mode }` → `null` (also marks the paper opened now) | create |
| `workspace.add_bookmark` / `remove_bookmark` | `{ name, location }` → `Bookmark` / `{ id }` → `null` | create |
| `workspace.record_reading` | `{ paper, page, seconds }` → `ReadingStats` (at most 300 s counted per report) | create |
| `workspace.reading_stats` | `{ paper }` → `ReadingStats { seconds, pages_read, last_ms? }` | read |

`workspace.list_papers` takes `{ limit?, offset, collection?, unfiled, tag?, query?, sort: added | opened | title }`; `query` words match title, authors, DOI or file name. `workspace.add_file` takes an optional `source_url` (http/https/mailto only) for files downloaded from the web.

`ProjectInfo { id, name, kind, folder, format, created_ms, papers }`
`Paper { id, file: BlobRef, original_name, original_path?, source_url?, title?, authors, doi?, pages?, metadata_source?, tags, added_ms, opened_ms? }`

Error kinds: `no_project_open`, `not_found`, `unsupported_file`, `folder_in_use`, `bad_format`, `exists` (collection name taken), `conflict` (notes changed meanwhile), `io`.

## Events (hub)
- `workspace.opened` (`ProjectInfo`), `workspace.paper_added` (`Paper`), `workspace.paper_changed` (`Paper`), `workspace.notes_changed` (`{ paper, version }`).

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
- 0.6.0 (2026-11-24): `save_digest`, `get_digests`.
- 0.5.0 (2026-11-24): folders inside folders, several papers at once, migration 7.
- 0.4.0 (2026-11-14): paper notes — `get_notes`, `save_notes`, `notes_history`, `restore_notes`; migration 5 (P1.MD.NOTE.1).
- 0.3.0 (2026-11-14): `set_paper_file` and migration 4 (`paper_version`); `Workspace::project_folder` and `store_blob` for the hosts' Document port (P1.M3.PDF.3).
- 0.2.0 (2026-11-13): collections, tags, library search and sort, reading state, bookmarks, reading time, source address (P1.M2.WSP.1–3, SH.7); migrations 2–3.
- 0.1.0 (2026-11-13): first slice (P1.M1.WSP.1–3).
