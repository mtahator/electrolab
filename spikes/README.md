# Spikes (Phase 0, work package B)

Throw-away programs that answer one risky question each. Spike code is **never** copied into the product; the lesson is, through an ADR.

| Spike | Question | Folder | Result | ADR |
|---|---|---|---|---|
| P0.S1 | Tauri + PDF.js on WebView2: smooth enough? | `s1-tauri-pdfjs/` | **pass** — see [RESULTS](s1-tauri-pdfjs/RESULTS.md); PDFium bitmap for heavy pages; selection text from the Document engine | ADR-003 |
| P0.S2 | PDFium text and geometry good enough? | `s2-pdfium/` | **pass** — see [RESULTS](s2-pdfium/RESULTS.md); RTL re-ordering and surrogate pairing required | ADR-003 |
| P0.S3 | llama.cpp on Windows: fast start, structured output? | `s3-llamacpp/` | **pass** — see [RESULTS](s3-llamacpp/RESULTS.md); ready in 1.4–3.3 s (vLLM 77.6 s); Qwen3-4B default proposed; checked Explain 2.8 s vs 2 s budget open | ADR-005 |
| P0.S4 | Annotations accepted by Acrobat, Edge, Chrome? | `s4-annotations/` | **pass** with our own incremental writer (lopdf) — see [RESULTS](s4-annotations/RESULTS.md); Acrobat desktop check open | ADR-006 |
| P0.S5 | Layout, formula, OCR models on CPU within budget? | `s5-page-models/` | **pass** — see [RESULTS](s5-page-models/RESULTS.md); Egret-medium 20 pages in 7–9 s; OCR as background stage; reading order and Persian OCR open | ADR-007 |
| P0.S6 | Vector index size vs installer budget | `s6-vector-index/` | **pass** — see [RESULTS](s6-vector-index/RESULTS.md); exact pure-Rust scan 6 ms / 100k, no index; LanceDB not adopted | ADR-008 |
| P0.S7 | Bus over local IPC < 2 ms | `s7-ipc/` | **pass** — see [RESULTS](s7-ipc/RESULTS.md); 0.05 ms round trip; Job Objects and no handle inheritance required | ADR-009 |

Each spike folder holds: `README.md` (question, method, hardware, files used), the code, and `RESULTS.md` (numbers, pass/fail, chosen fall-back).
Measure on this laptop with the fixture papers so results are comparable.
