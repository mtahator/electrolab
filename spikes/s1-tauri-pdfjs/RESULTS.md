# Spike P0.S1 — Results

Date: 2026-11-12 · Tauri 2.12.5 on WebView2 157 · PDF.js 6.4.299 · this laptop (60 Hz display, 1.75× scaling, 1265×871 viewport, pages at fit-width scale 2) · full tables: [out/report.md](out/report.md)

## Verdict: **pass, with one hybrid rule** — PDF.js displays pages; PDFium renders the rare heavy page in the background

| Criterion (phase-0 plan) | Result | Status |
|---|---|---|
| 100-page paper scrolls at 60 fps with virtualised pages | Both 100-page theses: **59.4–59.9 fps** at 1,200 and 3,000 px/s; frames > 33 ms ≤ 0.22%; **no frame showed a blank page** | **pass** |
| First page of a ~30-page PDF < 300 ms | Warm, 6 papers: first page drawn in **138–204 ms** (median per paper), text layer ready by **155–245 ms** (Persian 434 ms). Cold (fresh process, includes starting the PDF.js worker): 177–257 ms | **pass** |
| PDF loaded through a custom protocol with range requests | `ep` protocol answers `Range` with `206`; PDF.js reads through `PDFDataRangeTransport`. To show page 1 it read **3–30%** of typical papers (276 KB of a 9.4 MB paper in 4 requests); scanned NACA report 64% | **pass** |
| Text selection and double-click word pick work | Real Windows mouse input (`real-input.ps1`, events arrive as trusted): **double-click selected exactly the intended word on all 3 papers** (English, PLOS, Persian). Drag across an English line selected exactly that line. Programmatic word-pick test: 12/12 words on 5 papers, 11/11 on the Persian page | **pass for Latin text**; see findings 2–4 for the limits |

## The one problem: very heavy pages freeze the screen

On the heaviest fixture paper (2106.03711, 31 pages), PDF.js needs **2.4 s** to draw page 21, in main-thread chunks of up to **0.6 s**. While scrolling past pages 21–22 the window freezes for about half a second, several times. Averages hide it: the paper still scrolls at 53–57 fps, and only 0.2–0.9% of frames exceed 33 ms. A thesis has pages that take 0.9–1 s, without visible freezes in this run. The scanned NACA report drops 3–4% of frames while page images decode.

**PDFium renders the same pages much faster, and off the screen's thread** (`spikes/s2-pdfium/examples/render_time.rs`, same pixel size, 2142×2772):

| Page | PDF.js drawing (main thread) | PDFium load + render (background) |
|---|---|---|
| 2106.03711 p21 | 2,409 ms | 383 ms |
| 2106.03711 p25 | 510 ms | 202 ms |
| 2511.21915 p34 | 973 ms | 507 ms |
| 2511.21915 p53 | 896 ms | 187 ms |
| NACA p35 (scan) | 256 ms | 84 ms |

**Decision (hybrid):** PDF.js draws pages by default, so the text layer and selection stay native. When a page's drawing takes longer than a threshold (start with 150 ms, or a high operator count known in advance), the viewer cancels it and shows a PDFium bitmap rendered on the Document engine's thread instead, keeping the PDF.js text layer on top. The page-level "heaviness" is remembered per paper, so the second visit goes straight to PDFium. A full PDFium-tiles viewer (the plan's fall-back) is not needed.

## Findings for the Reader (go into Phase 1, M2)

1. **Lay out from PDFium sizes.** `page_sizes()` returns every page size in 4–8 ms without loading pages, so the whole document is laid out before PDF.js has parsed anything. Scroll height never jumps. Keep this design (P1.M2.VW.1).
2. **Selection follows file order, not visual order.** On the PLOS paper, a drag from the sidebar into the body selected a large block of the page, because the PDF.js text layer selects in content-stream order. Cross-column and sidebar selection needs our own handling (P1.M2.VW.5): re-order the text-layer DOM by the layout model's reading order, or draw selection from Document-engine quads.
3. **Persian text in the text layer is fragmented and in visual order.** PDF.js split one Persian line into 73 pieces, often mid-word, using Arabic presentation forms ("ﺪﺣ" for "حد"). Double-click worked on a short word inside one piece; longer words can't be relied on. The selected string is unusable as text. Rule: **text for copy, Explain, notes and evidence always comes from the Document engine** (PDFium quads plus the RTL fix found in spike S2), never from the browser's selection string. The DOM selection is only used to know *where* the user selected.
4. **The text layer is the slow part for Persian** (434 ms after a 176 ms page). Building it can wait until the page is idle, so it doesn't count against first paint.
5. **Range loading works but is not free for scans.** Image-heavy pages need most of their bytes; prefetch the next page's ranges during idle time.
6. **Memory stays bounded.** With canvases released beyond ±6 pages, 8–10 live canvases and a 6–40 MB JS heap after scrolling 100 pages. A full-size canvas at 1.75× is about 22 MB of pixels, so the release distance is the main memory control (budget: < 150 MB per 30-page PDF; whole-process memory not measured here).
7. **Jumping to a page** renders it in 54–76 ms (median), worst 660 ms (a scanned page whose image had not been fetched yet).
8. **Real-input testing works on WebView2.** Genuine mouse events from a script reach the page as trusted events, and the page can report what was selected. This is the pattern for the Reader's end-to-end tests of selection, double-click, drag and annotation tools.

## Practical facts

- Release executable 12.9 MB (Tauri + PDFium bindings, without `pdfium.dll` and without PDF.js assets); PDF.js assets 6.9 MB uncompressed (`pdf.mjs`, `pdf.worker.mjs`, cmaps, fonts, wasm, ICC profiles). Within the < 80 MB installer budget.
- Content-Security-Policy with no remote origins works with PDF.js 6 (worker from `'self'`, wasm decoders, cmaps and fonts vendored).
- Custom protocol on Windows is reached as `http://ep.localhost/…`; it needs CORS headers because the app origin is `http://tauri.localhost`.

## Not measured here

- Whole-process memory (WebView2 runs in separate processes); measured in M2 with the real viewer.
- Zoom gestures and re-render quality.
- macOS and Linux web views (port stage).
