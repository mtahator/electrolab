# Spike P0.S4 — Annotations that other readers accept

**Question (plans/phase-0, work package B):** can we write highlight, underline, strike-out, note with reply, free text, ink, rectangle and arrow as standard PDF annotations, with an incremental save that leaves the original bytes intact, so that Acrobat, Edge and Chrome show them, and can we read annotations made elsewhere?
**Fall-back named in the plan:** write the missing types with a pure-Rust PDF writer (`lopdf`).

**Throw-away code.** Lessons go into ADR-006 and Phase 1, M3.

## Files

| File | Does |
|---|---|
| `src/main.rs` | `support` — which subtypes PDFium can create · `write` — our writer: all kinds on page 1 as an incremental update (lopdf), markup placed on real words found by PDFium · `read` — PDFium reads every annotation back · `visible` — pixels changed per annotation, rendered with vs without annotations |
| `verify_mupdf.py` | Independent parser/renderer (MuPDF via PyMuPDF; measurement only): structure, repair warnings, reply links, status, arrowheads, pixel check, PNG |
| `verify_pdfjs.mjs` | PDF.js's view of the annotations (what browser viewers see) |
| `foreign_marks.py` | Marks written by another tool (MuPDF's writer) to test our reader |
| `capture-window.ps1` | Screenshot of one window (the Edge test window) |

## Reproduce

```
cargo xtask fetch-natives && cargo xtask fetch-fixtures
cd spikes/s4-annotations && cargo build --release
copy ..\..\.fixtures\papers\arxiv-2111.12066v2.pdf out\original.pdf
target\release\s4-annotations support ..\..\.natives\pdfium\bin
target\release\s4-annotations write   ..\..\.natives\pdfium\bin out\original.pdf out\annotated.pdf
target\release\s4-annotations read    ..\..\.natives\pdfium\bin out\annotated.pdf
target\release\s4-annotations visible ..\..\.natives\pdfium\bin out\annotated.pdf 1
uv run --no-project --with "pymupdf==1.26.*" python -I -X utf8 verify_mupdf.py out\annotated.pdf out
node verify_pdfjs.mjs out\annotated.pdf out\verify-pdfjs.json
uv run --no-project --with "pymupdf==1.26.*" python -I -X utf8 foreign_marks.py out\original.pdf out\foreign.pdf
```

Results: [RESULTS.md](RESULTS.md).
