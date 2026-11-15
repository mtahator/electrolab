# Working on the Document engine

## Purpose
Turn PDFs (later other documents) into positioned, searchable content for every host. The contract is [CONTRACT.md](CONTRACT.md); decisions are ADR-003 (PDFium extracts and writes, PDF.js displays), ADR-006 (annotations), ADR-007 (page models), ADR-013 (OCR: PP-OCRv5 everywhere, shipped with the app), ADR-015 (formulas: LaTeX-OCR, optional download).

## Never change without a version bump
- Stable command inputs/outputs and guarantees in CONTRACT.md (everything is experimental today).
- The coordinate system: page space, points, top-left origin, rotation applied (ADR-003).

## Rules
- `core/` is pure: no files, clock, randomness, network, environment. Timing (`elapsed_us`) is filled in by the facade.
- PDFium is called only from the adapter, on its one owning thread.
- Other engines get Document data only through `ep-document-contract`.
- ONNX Runtime is called only from `ep-document-adapter-onnx`; models are found through `models.rs` (pinned folder, file, size and SHA-256), never by a path typed elsewhere.
- Text for copy, Explain, notes and evidence comes from this engine (PDFium characters), never from the browser's selection string (spike S1).

## How to check your work
- `cargo nextest run -p ep-document-contract -p ep-document-core -p ep-document-adapter-pdfium -p ep-document` (needs `cargo xtask fetch-natives pdfium`; adapter tests skip without it)
- `cargo clippy -p ep-document-core --all-targets -- -D warnings`
- `cargo xtask arch-check`
- Page models: `cargo xtask fetch-natives onnxruntime` and `cargo xtask fetch-models` (OCR models; `--all` adds the 78 MB layout model). `tests/intel.rs` runs the OCR test with the bundled models and the layout test when `EP_MODELS_DIR` holds the layout model.
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

- **Rotation:** `PDFium` renders with `/Rotate` applied; our page space applies it too (`PageGeom`). `tests/robustness.rs` checks that both agree on a rotated copy of a fixture.
- **Math-font text is not the formula:** PDFium returns flattened subscripts, private-use bracket glyphs and control characters for equations. Never show it as the equation; show the picture or LaTeX (`docs/design/equations-figures-tables.md`).
- **OCR speed:** recognition is about 11 s per scanned page on 8 cores; more threads make it slower (16 threads: 31 s). Keep 4.
- **PDFium starts once per process:** a second start crashes it; `PdfiumBackend::start` shares the first worker.
- **Image-only pages** are judged on the PDF's own text, not on cached OCR text, or a page would stop counting as scanned once recognised.
- **Damaged files:** `PDFium` repairs some damage (a truncated file may still open); callers get either a document or a typed `Unreadable`, never a panic.

## Decisions
- ADR-003, ADR-006, ADR-007, ADR-013; design note `docs/design/document-search.md`.
