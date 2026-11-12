# Spike P0.S2 — Is PDFium text and geometry good enough?

**Question (plans/phase-0, work package B):** on the 20 fixture papers, is PDFium's text equal or better than the current extractor, are character boxes present, is text of 100 pages extracted in < 2 s, and do PDFium boxes agree with the PDF.js text layer within 1 pt?

**Throw-away code.** Nothing here is copied into the product; the lessons go into ADR-003 and the Document engine's contract and `DEVELOPING.md`.

## Method

| Part | Tool | Output |
|---|---|---|
| Extraction | `src/main.rs` — `pdfium-render` 0.9.5 on PDFium `chromium/8125` (pinned in `natives.toml`) | per paper: page texts, every character with baseline origin and loose box (PDF user space), timings |
| Text reference | `compare/pymupdf_ref.py` — PyMuPDF 1.26, called exactly as ElectroChat 2.5 does (`get_text("text", sort=False)`) | page texts, timings. Measurement only: PyMuPDF is AGPL and never ships |
| Text comparison | `compare/compare_text.py` | word-multiset F1, in-order similarity, defect counts per extractor |
| Geometry comparison | `compare/compare_geometry.mjs` — `pdfjs-dist` 6.4.299 | for each PDF.js text item: distance from its start/end to the matching PDFium character |
| Summary | `compare/report.py` | `out/report.md` |

There is no ground-truth text, so text quality is judged by (a) agreement with the current extractor and (b) defects that are wrong whatever the truth is: replacement characters, unexpanded ligatures, merged words, single-letter fragments.

**Hardware:** this laptop (the plan's minimum hardware) — Windows 11, 16 GB RAM. Release build, single thread, warm disk cache.

## Reproduce

```
cargo xtask fetch-natives
cargo xtask fetch-fixtures
cd spikes/s2-pdfium && cargo build --release
target/release/s2-pdfium ../../.natives/pdfium/bin out <pdfs…> > out/pdfium.jsonl
cd compare && npm install
uv run --no-project --with "pymupdf==1.26.*" python -I -X utf8 pymupdf_ref.py ../out <pdfs…> > ../out/pymupdf.jsonl
node compare_geometry.mjs ../out <pdfs…>
python -I -X utf8 compare_text.py ../out
python -I -X utf8 report.py ../out/pdfium.jsonl ../out/pymupdf.jsonl ../out > ../out/report.md
```

`<pdfs…>` = `.fixtures/papers/*.pdf` plus `fixtures/papers/fawiki-artificial-neural-network.pdf`.

Results: [RESULTS.md](RESULTS.md).
