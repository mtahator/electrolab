# Spike P0.S1 — Tauri + PDF.js on WebView2: smooth enough?

**Question (plans/phase-0, work package B):** with the real viewer path — Tauri 2, WebView2, PDF.js, virtualised pages, PDF loaded through our own protocol with range requests — does a 100-page paper scroll at 60 fps, does the first page of a ~30-page PDF appear in < 300 ms, and do text selection and double-click word picking work?
**Fall-back if not:** display PDFium-rendered tiles, with selection from PDFium text boxes.

**Throw-away code.** Lessons go into ADR-003, `technical-structure.md §7` and the Reader tasks of Phase 1.

## What the spike app does

| Part | Where | Mirrors in the product |
|---|---|---|
| `ep` protocol (`http://ep.localhost/file?path=…` on Windows) serving bytes with `Range` → `206 Partial Content`; only fixture folders allowed | `src-tauri/src/main.rs` | `ep://blob/<hash>` (technical-structure.md §7.5) |
| PDF.js fed through `PDFDataRangeTransport` using that protocol (64 KB chunks, no auto-fetch) | `ui/app.js` | Reader viewer |
| Every page size from PDFium (`page_sizes()`, no page loading) before PDF.js parses anything, so the whole document is laid out at once | `doc_info` command | Document engine supplies sizes (§7.6) |
| Virtualised continuous scroll: visible pages first, 2 pages prefetched below, renders cancelled when far away, canvases released beyond ±6 pages, at most 2 renders in flight | `ui/app.js` | `viewer/` render queue (P1.M2.VW.1–2) |
| Canvas at device pixel ratio + PDF.js text layer per page | `ui/app.js` | `layers/` (P1.M2.VW.5) |
| Measurements, run in the page and written to `out/*.json` | `ui/app.js`, `run-bench.ps1`, `summarise.py` | — |

### Measurements

- **open** — one cold open (first in a fresh process, includes starting the PDF.js worker) and 5 warm re-opens; time to first page canvas and to its text layer; bytes and requests through the protocol; a word-pick test: for 12 words on page 1, the point at the word's centre must hit that word in the text layer, and a word selection from there (what double-click does) must return it.
- **scroll** — scroll continuously from page 1 to page N at 1,200 px/s and 3,000 px/s, driven by `requestAnimationFrame`; frame times, frames showing an unrendered page, long tasks and which page/phase caused them.
- **jump** — jump to 10 pages spread through the document; time until the page is fully rendered.

## Reproduce

```
cargo xtask fetch-natives
cargo xtask fetch-fixtures
cd spikes/s1-tauri-pdfjs
npm install && npm run vendor
cd src-tauri && cargo build --release && cd ..
pwsh ./run-bench.ps1            # windows open and close by themselves; keep the PC otherwise idle
python -I -X utf8 summarise.py  # → out/report.md
```

Manual mode: run `src-tauri/target/release/s1-tauri-pdfjs.exe` without arguments and pick a paper from the list.

Results: [RESULTS.md](RESULTS.md).
