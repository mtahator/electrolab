# ADR-003: PDFium for extraction and writing, PDF.js for display, one coordinate system

Status: **extraction part accepted** (spike S2) · display part proposed, pending spike S1
Date: 2026-11-12
Plan reference: Draft 2.2 §14.2, §21, §28.7; technical-structure.md §7.4, §7.6; tasks P0.S1, P0.S2

## Context
The Document engine needs text with exact positions for search, evidence, highlights and region detection, from every host (Reader, CLI, Python, MCP). The Reader also needs smooth on-screen display with selectable text. ElectroChat 2.5 uses PyMuPDF, which is AGPL and cannot ship in a closed product (§21 licence note).

## Decision
1. **PDFium** (via `pdfium-render`, prebuilt binaries pinned in `natives.toml`) is the only extractor in the Document engine: text, character geometry, links, outline, and later annotation writing. It runs on one owning thread.
2. **PDF.js** displays pages and provides the selectable text layer in the Reader UI (to be confirmed by spike S1).
3. **One coordinate system** for everything that crosses a boundary: page space in points (technical-structure.md §7.4). Spike S2 measured PDFium and PDF.js text origins agreeing within 1 pt for 100% of matched items on 19/20 papers (97.6% on Persian), so geometry from PDFium lines up with the text the user selects in PDF.js.

## Alternatives considered
| Option | Why not |
|---|---|
| Keep PyMuPDF (as in ElectroChat) | AGPL; not better on text (median agreement 0.95, more unexpanded ligatures, raw control codes for unmapped glyphs); similar speed |
| MuPDF via Rust bindings | AGPL, same problem |
| Pure-Rust PDF parsers | Text extraction quality and font handling far behind PDFium on real papers |
| PDF.js for extraction too | Would tie extraction to a JavaScript runtime; CLI, Python and MCP hosts need it without a web view |

## Consequences
- The PDFium adapter must: pair UTF-16 surrogates (math alphanumerics), turn the `\x02` soft-hyphen marker into a joined word, and treat `U+FFFD` and private-use glyphs as unknown glyphs. The Document core must re-order right-to-left text (required for Persian, P1.M2.DOC.3). Details: `spikes/s2-pdfium/RESULTS.md`.
- Page loading dominates extraction time (about 80–95% in slower papers); visible page first, background preparation, never on the UI thread.
- 7.6 MB library (3.9 MB compressed) per OS; builds exist for Windows, macOS and Linux.
- PDFium version changes are pinned upgrades with the S2 comparison re-run as a regression check.

## Evidence
`spikes/s2-pdfium/RESULTS.md`, `spikes/s2-pdfium/out/report.md` — 20 fixture papers, 915 pages, minimum hardware.
