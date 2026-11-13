# Workspace engine — contract

Version: 0.1.0      Stability: experimental      Tier: T1
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
| `workspace.add_file` | `{ path }` → `{ paper: Paper, already_present }` | create |
| `workspace.get_paper` | `{ id }` → `Paper` | read |
| `workspace.list_papers` | `{ limit?, offset }` → `[Paper]`, newest first | read |
| `workspace.set_paper_metadata` | `{ id, title?, authors?, doi?, pages?, source }` → `Paper` | create |

`ProjectInfo { id, name, kind, folder, format, created_ms, papers }`
`Paper { id, file: BlobRef, original_name, original_path?, title?, authors, doi?, pages?, metadata_source?, added_ms }`

Error kinds: `no_project_open`, `not_found`, `unsupported_file`, `folder_in_use`, `bad_format`, `io`.

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
- 0.1.0 (2026-11-13): first slice (P1.M1.WSP.1–3).
