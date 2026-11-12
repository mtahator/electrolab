# ADR-002: Windows first, portable by design

Status: accepted
Date: 2026-11-12
Plan reference: Draft 2.2 §2.5, §6.4, §24 Stage 6; task P0.D

## Context
The founder's machine and most target researchers' laptops run Windows. Supporting three operating systems from the start would slow T1. Porting later must not become a rewrite.

## Decision
T1 is built, tested and shipped on Windows 10/11 x64 only. The twelve rules of §6.4 apply from the first commit, in particular:

1. OS-specific code only in adapters (`engines/*/adapters/`, `foundation/os-*`) — checked by `cargo xtask arch-check`.
2. Ports exist now for everything OS-dependent (`Sandbox`, `ProcessSpawner`, `Accelerators`, `KeyStore`, `AppDirs`, `FileWatcher`, `Updater`, `Notifier`, `ShellOpen`, `FileDialog`).
3. Compile on three, test and ship on one: CI runs `cargo check` and core/contract tests for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` (targets pinned in `rust-toolchain.toml`).
4. `PathBuf` only, relative `/` paths in project files; UTF-8 and LF (`.gitattributes`); processes spawned with argument vectors; no WSL dependency; native dependencies chosen to exist on all three OSes.

## Alternatives considered
| Option | Why not |
|---|---|
| Three OSes in T1 | Triples packaging, sandbox and GPU work before any user feedback |
| Windows only, port "later" without rules | Windows assumptions spread through the code; the port becomes a rewrite |

## Consequences
- Stage 6 (Phase 3 Part A) is mostly new adapters, packaging and CI.
- A small, permanent CI cost for the Linux and macOS check jobs.
- Toolchain on this PC: Rust 1.99.1 (MSVC) and Visual Studio 2022 Build Tools (C++ workload), chosen over Build Tools 2026 for the widest crate and Tauri support.
