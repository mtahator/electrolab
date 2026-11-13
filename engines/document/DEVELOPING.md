# Working on the Document engine

## Purpose
Turn PDFs (later other documents) into positioned, searchable content for every host. The contract is [CONTRACT.md](CONTRACT.md); decisions are ADR-003 (PDFium extracts and writes, PDF.js displays), ADR-006 (annotations), ADR-007 (page models).

## Never change without a version bump
- Stable command inputs/outputs and guarantees in CONTRACT.md (everything is experimental today).
- The coordinate system: page space, points, top-left origin, rotation applied (ADR-003).

## Rules
- `core/` is pure: no files, clock, randomness, network, environment. Timing (`elapsed_us`) is filled in by the facade.
- PDFium is called only from the adapter, on its one owning thread.
- Other engines get Document data only through `ep-document-contract`.
- Text for copy, Explain, notes and evidence comes from this engine (PDFium characters), never from the browser's selection string (spike S1).

## How to check your work
- `cargo nextest run -p ep-document-contract -p ep-document-core`
- `cargo clippy -p ep-document-core --all-targets -- -D warnings`
- `cargo xtask arch-check`
- `cargo run -p ep-document-core --example search_bench --release -- spikes/s2-pdfium/out` (needs `cargo xtask fetch-fixtures` and spike S2's output)

## Known pitfalls (from spikes S1, S2, S4, S5)
- **UTF-16 surrogates:** PDFium returns math alphanumerics (𝐻, 𝑘) as two "chars"; pair them in the adapter (S2).
- **`\x02`** is PDFium's soft-hyphen marker: the word continues after the line break (handled by the search tokenizer).
- **Right-to-left text** comes out with letters in visual order in some words; re-order in the core (S2: Persian 0.41 → 0.94).
- **U+FFFD, private-use glyphs and control codes** are unknown glyphs, not text; never quote them as evidence.
- **Page loading dominates** extraction time (80–95% in slow papers); load the visible page first.
- **pdfium-render cannot save incrementally, write Line annotations or reply links** — annotation export uses our own lopdf writer (ADR-006).
- **Notation search:** punctuation must never join symbol parts (`C. P. Burgess` is not `C_p`); phrases may cross spaces, line breaks and hyphens only.
- **f32 rounding:** `(1 − 0.8) × 10` is `1.9999999`; edit budgets need a small tolerance.

## Decisions
- ADR-003, ADR-006, ADR-007; design note `docs/design/document-search.md`.
