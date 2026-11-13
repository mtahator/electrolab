# ADR-003: PDFium for extraction and writing, PDF.js for display, one coordinate system

Status: **accepted** — extraction (spike S2), display with a hybrid rule for heavy pages (spike S1)
Date: 2026-11-12
Plan reference: Draft 2.2 §14.2, §21, §28.7; technical-structure.md §7.4, §7.6; tasks P0.S1, P0.S2

## Context
The Document engine needs text with exact positions for search, evidence, highlights and region detection, from every host (Reader, CLI, Python, MCP). The Reader also needs smooth on-screen display with selectable text. ElectroChat 2.5 uses PyMuPDF, which is AGPL and cannot ship in a closed product (§21 licence note).

## Decision
1. **PDFium** (via `pdfium-render`, prebuilt binaries pinned in `natives.toml`) is the only extractor in the Document engine: text, character geometry, links, outline, and later annotation writing. It runs on one owning thread.
2. **PDF.js** displays pages and provides the selectable text layer in the Reader UI. Spike S1: 100-page papers scroll at 59–60 fps, first page in 138–204 ms, range loading through our protocol works, and real double-clicks select the right word.
   **Hybrid rule:** when PDF.js takes longer than a threshold (start: 150 ms) to draw a page, the viewer shows a PDFium bitmap rendered on the Document engine's thread instead, keeping the PDF.js text layer on top, and remembers the page as heavy. (Heaviest fixture page: PDF.js 2.4 s on the UI thread, with freezes up to 0.6 s; PDFium 0.38 s in the background.)
   **Text comes from the Document engine.** The browser selection only says *where* the user selected; the text for copy, Explain, notes and evidence is taken from PDFium quads (PDF.js gives Persian in visual order and presentation forms, and selects in file order across columns).
3. **One coordinate system** for everything that crosses a boundary: page space in points (technical-structure.md §7.4). Spike S2 measured PDFium and PDF.js text origins agreeing within 1 pt for 100% of matched items on 19/20 papers (97.6% on Persian), so geometry from PDFium lines up with the text the user selects in PDF.js.

## Alternatives considered
| Option | Why not |
|---|---|
| Keep PyMuPDF (as in ElectroChat) | AGPL; not better on text (median agreement 0.95, more unexpanded ligatures, raw control codes for unmapped glyphs); similar speed |
| MuPDF via Rust bindings | AGPL, same problem |
| Pure-Rust PDF parsers | Text extraction quality and font handling far behind PDFium on real papers |
| PDFium page tiles for all display (the plan's fall-back) | Not needed: PDF.js meets every budget on normal pages; PDFium is used only for heavy pages |
| PDF.js for extraction too | Would tie extraction to a JavaScript runtime; CLI, Python and MCP hosts need it without a web view |

## Consequences
- The PDFium adapter must: pair UTF-16 surrogates (math alphanumerics), turn the `\x02` soft-hyphen marker into a joined word, and treat `U+FFFD` and private-use glyphs as unknown glyphs. The Document core must re-order right-to-left text (required for Persian, P1.M2.DOC.3). Details: `spikes/s2-pdfium/RESULTS.md`.
- Page loading dominates extraction time (about 80–95% in slower papers); visible page first, background preparation, never on the UI thread.
- 7.6 MB library (3.9 MB compressed) per OS; builds exist for Windows, macOS and Linux.
- PDFium version changes are pinned upgrades with the S2 comparison re-run as a regression check.

## Evidence
`spikes/s2-pdfium/RESULTS.md`, `spikes/s2-pdfium/out/report.md` — 20 fixture papers, 915 pages, minimum hardware.
`spikes/s1-tauri-pdfjs/RESULTS.md`, `spikes/s1-tauri-pdfjs/out/report.md` — display, scrolling, range loading, real mouse input.

## Amendment 1 (2026-11-13): PDFium draws by default

**Measured in the built Reader** (`e2e/perf.mjs`; scroll the whole paper, 80 % of the view per frame):

| Drawing | Paper | Longest main-thread task | Frames over 33 ms |
|---|---|---|---|
| PDF.js (main thread) | 2106.03711 (S1 heavy, 31 p.) | 249–329 ms | 49–57 of 64 |
| PDF.js | 2511.21915 (286 p.) | 351–536 ms | 563–571 of 594 |
| PDFium images (render cache, decoded off the main thread) | 2106.03711 | 0–55 ms | 7 of 64 |
| PDFium images | 2511.21915 | 99 ms | 26 of 594 |

S1 measured PDF.js page by page in a simple viewer; a real viewer drawing several pages while scrolling blocks the main thread far beyond the 100 ms budget, because PDF.js draws canvases on the main thread (its worker only parses).

**Decision:** pages are drawn by PDFium on its own thread and shown as images from the render cache (`document.render`). PDF.js still loads the document (range requests) and draws only the recoloured themes (dark, night, high contrast), which need its page colours (text and lines recoloured, images untouched). Text, links and selection come from the Document engine in every case.

**Follow-up:** draw the recoloured themes with PDFium's colour-scheme rendering (`FPDF_RenderPageBitmapWithColorScheme_Start`: path and text colours replaced, images kept). pdfium-render does not expose the page and bitmap handles it needs, so this takes a small direct binding to the PDFium library in the adapter (a named `unsafe` boundary). Until then the recoloured themes scroll with PDF.js's main-thread cost.
