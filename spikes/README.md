# Spikes (Phase 0, work package B)

Throw-away programs that answer one risky question each. Spike code is **never** copied into the product; the lesson is, through an ADR.

| Spike | Question | Folder | Result | ADR |
|---|---|---|---|---|
| P0.S1 | Tauri + PDF.js on WebView2: smooth enough? | `s1-tauri-pdfjs/` | **pass** — see [RESULTS](s1-tauri-pdfjs/RESULTS.md); PDFium bitmap for heavy pages; selection text from the Document engine | ADR-003 |
| P0.S2 | PDFium text and geometry good enough? | `s2-pdfium/` | **pass** — see [RESULTS](s2-pdfium/RESULTS.md); RTL re-ordering and surrogate pairing required | ADR-003 |
| P0.S3 | llama.cpp on Windows: fast start, structured output? | `s3-llamacpp/` | — | ADR-005 |
| P0.S4 | Annotations accepted by Acrobat, Edge, Chrome? | `s4-annotations/` | — | ADR-006 |
| P0.S5 | Layout, formula, OCR models on CPU within budget? | `s5-page-models/` | — | ADR-007 |
| P0.S6 | Vector index size vs installer budget | `s6-vector-index/` | — | ADR-008 |
| P0.S7 | Bus over local IPC < 2 ms | `s7-ipc/` | — | ADR-009 |

Each spike folder holds: `README.md` (question, method, hardware, files used), the code, and `RESULTS.md` (numbers, pass/fail, chosen fall-back).
Measure on this laptop with the fixture papers so results are comparable.
