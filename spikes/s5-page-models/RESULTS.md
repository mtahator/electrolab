# Spike P0.S5 — Results

Date: 2026-11-13 · ONNX Runtime 1.31 (CPU) · Ryzen 7 5800H, 16 GB · pages rendered with PDFium (pypdfium2) · full tables: [out/report.md](out/report.md)

## Verdict: **pass for layout, OCR as a background stage, formulas lazily** — Docling **Egret-medium** for layout, **PP-OCRv5** (plus Windows OCR for the visible page) for scans, **LaTeX-OCR** for single-line formulas with crop fixes. Reading order and Persian OCR remain open.

| Criterion (phase-0 plan) | Result | Status |
|---|---|---|
| Text + outline + objects of a 20-page paper < 10 s on CPU | Layout with Egret-medium: **7.2–8.6 s for 20 pages with 4 threads** (0.36 s/page; 8 threads is no faster). Text extraction adds ~0.1 s (spike S2) | **pass** — with half the CPU left free |
| Objects found | Figures/tables next to the paper's own captions: **86/96**; numbered equations: **296/299** (same for all models; the misses are mostly multi-panel figures in one paper, a limit of the caption rule) | **pass** |
| Reading order | Not measured: layout models return boxes, not order | **open** → P1.M4.OBJ.1 |
| OCR of image-only pages | PP-OCRv5 mobile (English): **93–95 % word agreement** on text pages, 47–76 % on table pages, **1.8–7.6 s per page**. Windows built-in OCR: 91–93 % / 32–71 %, **0.14–0.24 s per page** | **pass** as a background stage |
| Formulas → LaTeX | LaTeX-OCR (pix2tex): single-line equations correct-looking, **median 1.1 s**; multi-line blocks 4–31 s; one very wide crop looped (18 s of `~~~~`) | **pass for single lines**, lazily, with crop fixes |
| Licences | Egret-medium, Heron, PP-DocLayout, PP-OCRv5: Apache-2.0; LaTeX-OCR weights MIT, wrapper Apache-2.0; Windows OCR is part of the OS | **pass** |

## Layout models compared (same 9 papers, first ≤ 20 pages)

| Model | Size | ms/page | 20-page paper | Captions | Equations |
|---|---|---|---|---|---|
| **Docling Egret-medium** (D-FINE) | 75 MB | **364** (4 threads) | **7.2–8.6 s** | 86/96 | **296/299** |
| Docling Heron (RT-DETRv2) | 163 MB | 599 | 12.2–13.7 s | 86/96 | 295/299 |
| PP-DocLayout_plus-L | 124 MB | 826 | 16.4–18.6 s | 86/96 | 286/299 |
| Heron, int8 dynamic quantisation | 50 MB | 6,306 | 2+ min | 86/96 | 287/299 |

Egret is fastest and as accurate as the larger models. **8-bit quantisation made the model 10× slower** (ONNX Runtime's integer convolutions are not optimised on this CPU) — not a route to speed here.

## Findings for the Document engine (Phase 1, M4)

1. **Layout: Egret-medium through `ort`, 4 intra-op threads, one shared session**, page rendered by PDFium at 144 dpi and resized to 640×640. The ONNX file in use is a third-party export of Docling's Apache-2.0 weights; for the product, export it ourselves from the official weights (reproducible, no third-party binary) and pin the hash.
2. **Heron's official ONNX file includes pre- and post-processing** (uint8 image in, boxes in page pixels out) — the simplest integration if Heron is ever needed for quality.
3. **OCR has two adapters behind the `Ocr` port:** Windows' built-in engine for the page in view (≈0.2 s), PP-OCRv5 in the background for best quality and for any language Windows lacks. PP-OCR needs DB box extraction and CTC decoding around the models (written in the spike, ~100 lines) and **width-sorted batching** (halved recognition time).
4. **Persian OCR is weak:** PP-OCRv5's Arabic-script model reaches ~60 % word agreement after reversing its visual-order output; Windows OCR could not be tested (the Persian OCR language pack is not installed — installing it changes a system setting). Persian scans need another model or engine before they can be promised.
5. **Formulas need clean crops:** cut off the equation number (PDFium already knows its position), trim blank space, split multi-line blocks into lines, cap output length and time. Equation numbers and simple inline math come from PDFium text (spike S2), not from the model. Stronger formula models (UniMERNet, PP-FormulaNet_plus; Apache-2.0) have no ready ONNX build — candidates for P1.M4.EQ.1.
6. **Scheduler rules hold:** layout uses ~half the cores; preparation must pause while the language model generates on CPU-only machines (P1.M4.JOB.2).
7. **Download sizes:** Egret 75 MB + PP-OCRv5 det/rec 12 MB per script + LaTeX-OCR ~170 MB — all downloaded on first use, not in the installer.

## Not measured here

- Reading order (open); tables to cells (P1.M4.TAB.1); rotated scans; quality on a hand-labelled layout set (the caption/equation rule is a proxy).
- The same pipelines in Rust (`ort`); speed should match ONNX Runtime from Python, to be confirmed in M4.
