# ADR-007: Page understanding on CPU — Egret-medium layout, two OCR adapters, LaTeX-OCR for single-line formulas

Status: **accepted** (layout) · **superseded for OCR by [ADR-013](ADR-013-open-offline-ocr.md)** (one open engine, bundled) · **proposed** (formula model; Persian OCR open)
Date: 2026-11-13
Plan reference: Draft 2.2 §14.2 (`LayoutModel`, `FormulaModel`, `Ocr` ports), §28.3 (Region tool, Scanned pages, Equations, Background preparation), §28.7; Phase 1 M4; task P0.S5

## Context
The Reader needs every figure, table and equation on a page as an object (region detection < 50 ms is a lookup on precomputed objects), scanned pages made searchable, and equations as LaTeX — prepared in the background on the CPU so the GPU stays free for the language model, within 10 s for a 20-page paper (text, outline, objects).

## Decision
1. **Layout model: Docling Egret-medium** (Apache-2.0, 75 MB) via ONNX Runtime (`ort`), 4 threads, pages rendered by PDFium. 20 pages in 7.2–8.6 s; finds 296 of 299 numbered equations and the same figures/tables as the larger Heron and PP-DocLayout. We export the ONNX file ourselves from the official weights and pin its hash.
2. *(Superseded by ADR-013: PP-OCRv5 only, on every system, shipped with the app.)* **OCR port with two adapters:** Windows built-in OCR (≈0.2 s/page) for the page in view; PP-OCRv5 mobile (Apache-2.0, ~12 MB per script) in the background and wherever the OS lacks the language. The PP-OCR pre/post-processing (DB boxes, CTC decoding, width-sorted batches) lives in the adapter. macOS (Vision) and Linux adapters at the port stage.
3. *(Accepted with details in ADR-015.)* **Formulas: LaTeX-OCR (pix2tex, MIT) run lazily** on single-line equations with clean crops (number cut off using PDFium positions, blank space trimmed, output length and time capped); multi-line blocks split into lines first. Re-evaluate UniMERNet / PP-FormulaNet_plus (Apache-2.0) in P1.M4.EQ.1 when an ONNX export is made.
4. **Deterministic first:** equation numbers, captions and inline symbols come from PDFium text; models fill what text cannot give.
5. *(Amended by ADR-013: the OCR models ship with the app; the layout and formula models are downloaded.)* **All models are downloaded on first use**, pinned by commit and SHA-256 (as in `spikes/s5-page-models/models.json`).

## Alternatives considered
| Option | Why not |
|---|---|
| Docling Heron | Same quality, 1.6× slower (12–14 s for 20 pages) |
| PP-DocLayout_plus-L | Slower (16–19 s) and slightly fewer equations found |
| int8 quantised models | 10× slower on this CPU with ONNX Runtime |
| YOLO-based layout models (e.g. DocLayout-YOLO) | AGPL/GPL licences |
| Only Windows OCR | Lower quality on tables; Windows-only; language packs must be installed |
| Only PP-OCR | 2–8 s per page — too slow for the page the user is looking at |

## Consequences
- Background preparation of a 20-page born-digital paper fits the 10 s budget using half the CPU; scanned papers need a longer background OCR stage (PP-OCR ≈ 2 min for 20 pages), with the visible page OCR'd at once by the OS engine.
- **Reading order (resolved in M4):** XY-cut over the boxes, columns before rows; used for page objects and OCR lines.
- **Open — Persian scans:** ~60 % word agreement with PP-OCR's Arabic-script model; Windows' Persian OCR untested (language pack not installed). No promise of Persian OCR quality until a better engine is measured.
- ~260 MB of model downloads for the full page-understanding set.

## Evidence
`spikes/s5-page-models/RESULTS.md`, `spikes/s5-page-models/out/report.md`.
