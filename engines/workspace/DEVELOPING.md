# Working on the Workspace engine

## Purpose
Own libraries and projects: folder format, managed copies, the paper table and the project's event log. Contract: [CONTRACT.md](CONTRACT.md); format: ADR-010 and `spec/project-format.md`.

## Never change without a version bump
- The folder format (`PROJECT_FORMAT`), the manifest fields and existing migrations. Add a migration; never edit one that shipped.
- Command names and the error kinds in `ep_workspace_contract::errors`.

## Rules
- `core/` is pure: format parsing, classification, SQL text. The facade does all I/O.
- Blocking work (hashing, copying, SQLite writes) runs in `spawn_blocking`.
- Never touch the user's original file except to read it.
- Workspace does not call other engines; the host chains `workspace.add_file` → `document.metadata` → `workspace.set_paper_metadata`.

## How to check your work
- `cargo nextest run -p ep-workspace-contract -p ep-workspace-core -p ep-workspace`
- `cargo clippy -p ep-workspace --all-targets -- -D warnings`
- `cargo xtask arch-check`

## Known pitfalls
- Long Windows paths come back from canonicalisation with `\\?\`; compare paths with the `ep_os::paths` helpers, not string prefixes.
- SQLite `user_version` is the schema version; a newer one means a newer app wrote the file.
