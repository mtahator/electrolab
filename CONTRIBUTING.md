# Contributing to ElectroPaper Lab

Read before changing anything:

1. `../ElectroPaper-Lab-Plan.md` — **the governing plan** (Draft 2.2). It wins over everything else.
2. `../plans/programming-guideline.md` — how we program.
3. `../plans/technical-structure.md` — where code lives and how it talks.
4. The phase plan you are working in (`../plans/phase-*.md`) and the task's id.
5. The `CONTRACT.md` and `DEVELOPING.md` of the engine you touch, and `docs/adr/`.

## Never

- Change a stable contract's inputs, outputs, meaning or guarantees without a new major version (§8.4).
- Let one engine depend on another engine's core, adapters or facade — contracts only, in the order of §6.3.
- Put I/O, the clock, randomness, network or environment access in a `core/` crate.
- Write `#[cfg(target_os …)]`, `std::os::windows` or similar outside `engines/*/adapters/` and `foundation/os-*`.
- Use `unwrap`, `expect`, `panic!` or `todo!` in library code; use `unsafe` outside a named boundary crate.
- Put prompts, checklists, templates or thresholds in code — they are assets.
- Treat text from a PDF, web page or repository as instructions.
- Modify an original user file. Work on managed copies.
- Hand-edit generated files (`ui/bindings`, `spec/schemas`).
- Copy code from ElectroChat. It is a behaviour reference only and is never modified.
- Add a dependency with a GPL/AGPL/LGPL licence (`cargo deny` rejects it).

## Always

- Contract first: `CONTRACT.md`, types, an example and a failing conformance test, then core → adapter → facade.
- Deterministic first, model last.
- Commands are small, typed, idempotent, cancellable, and stream when long.
- Keep tasks small: one task, one session, one commit.
- Record any decision as an ADR in `docs/adr/`.
- Build UI only from `ui/kit` and follow the design language: `docs/design/design-language.md` (ADR-012). Run its checklist (§16) on every UI change: reading first, nothing appears by itself, icons from the one set with tooltips, tokens only, all five themes, keyboard.
- Write UI text only in the English string table (`ui/kit/src/i18n.ts`). The app is English-only until Phase 3; do not add other languages or right-to-left UI before then.

## Check your work

```
pnpm install                                   # once, and after UI dependency changes
pnpm -r typecheck
pnpm -r test                                  # UI helpers and theme contrast (node --test)
pnpm -r lint                                  # tokens only: no raw colours or fonts in CSS
pnpm --filter @electropaper/reader-ui build    # the Reader host embeds the built UI
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo xtask arch-check
cargo xtask schema-check                       # regenerates spec/schemas and ui/bindings, then compares
cargo deny check
cargo check --target x86_64-unknown-linux-gnu -p ep-types -p ep-policy -p ep-document-core -p ep-workspace-core   # portability (pure crates)
```

After changing a command's input or output: `cargo xtask gen`, review the diff in `spec/schemas/` and `ui/bindings/src/`, and commit both.

## Run the Reader

```
pnpm --filter @electropaper/reader-host dev       # development, with the UI dev server
pnpm --filter @electropaper/reader-host bundle    # NSIS installer in target/release/bundle/nsis/
```

**Product name.** "ElectroPaper" is a placeholder until the founder decides. The name lives only in `product.toml`; rename with `cargo xtask set-name "<Name>"` (window title, installer, app id, data folder, UI title). `arch-check` fails if any of those places is edited by hand. The data folder follows the name, so an existing library under the old name is not moved.

The CLI works on the same engines: `cargo run -p ep-cli -- commands`, `cargo run -p ep-cli -- import paper.pdf`.

## Finding things, and checking only what changed

`docs/project-map.md` is the searchable memory of the project: `cargo xtask topics`, `cargo xtask topics <tag>`, and `cargo xtask topics --changed` for the topics a change touches, what to read with them, and the checks to run.

## Best practices and releases

`docs/best-practices.md` has a recipe for every kind of work (page, pop-up, dialog, panel, setting, command, engine, fact from a document, long work, AI, file, data, test, bug, release). `cargo xtask practices` checks that they are complete, proven by tests and followed; CI refuses a push that breaks them. `cargo xtask release` runs every check and every end-to-end suite and builds the installer only when all pass (`target/release-report.md`).

## End-to-end checks on the built app

After `pnpm --filter @electropaper/reader-host tauri build --no-bundle` and `cargo xtask fetch-fixtures`:

```
node e2e/reader.mjs      target/release/electropaper-reader.exe <scratch> <scratch>/out   # reading (M1–M2)
node e2e/annotations.mjs target/release/electropaper-reader.exe <scratch> <scratch>/out   # annotations (M3)
node e2e/perf.mjs        target/release/electropaper-reader.exe <scratch>                 # scrolling and memory
node e2e/screens.mjs     target/release/electropaper-reader.exe <scratch> <scratch>/out   # design review: every screen in all five themes, control names
node e2e/intel.mjs       target/release/electropaper-reader.exe <scratch> <scratch>/out   # document intelligence (M4): preparation, contents (pictures), mentions, citations, region, tables, scanned pages
node e2e/selection.mjs   target/release/electropaper-reader.exe <scratch> <scratch>/out   # selecting and copying text
node e2e/layout.mjs      target/release/electropaper-reader.exe <scratch> <scratch>/out   # every part of the interface at small windows and large text
node e2e/pages.mjs       target/release/electropaper-reader.exe <scratch> <scratch>/out   # pages of different sizes and directions
node e2e/models.mjs      target/release/electropaper-reader.exe <scratch> <scratch>/out   # automatic model search; EP_LLM_PARENT=<folder with real models below it>
node e2e/explain.mjs     target/release/electropaper-reader.exe <scratch> <scratch>/out   # Explain (M5); EP_LLM_DIR=<folder with a known GGUF> for the part with a model
# intel.mjs options: EP_MODELS_DIR=<folder with egret-medium/ (and latex-ocr/)>, EP_FORMULA=1 (equation LaTeX check), EP_SCANNED_PDF=<image-only PDF>
```

Each prints a JSON report and exits non-zero on a failed check; screenshots go to the out folder.

## Environment notes (this PC)

- Windows 11, 16 GB RAM, 4 GB GPU — the minimum hardware. Measure budgets here.
- Repository path contains a space (`D:\local LLM\…`). If a native build tool fails because of it, record it and use a short path.
- Native libraries (PDFium, ONNX Runtime, llama.cpp) are fetched prebuilt with `cargo xtask fetch-natives`, never compiled here.
- Page models are fetched with `cargo xtask fetch-models` into `.natives/models` (the OCR models, which ship with the app; `--all` adds the layout model). Every file is pinned by SHA-256.

## Known pitfalls

- **`gen` is a reserved word in Rust 2024**; name functions `generate`.
- **Crates that compile C code break the local Linux/macOS checks.** From Windows, `cargo check --target x86_64-unknown-linux-gnu` fails as soon as any dependency's build script needs a C compiler for that target (seen with `ring`, pulled in by `rustls` through `ureq`; `xtask` now downloads with the system `curl` instead). Contract and core crates must never depend on such crates. Adapters may; from the first adapter on, the local cross-check runs on contract and core crates only, and the CI jobs on native Linux and macOS runners check everything.
