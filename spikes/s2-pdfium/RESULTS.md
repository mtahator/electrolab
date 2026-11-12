# Spike P0.S2 — Results

Date: 2026-11-12 · PDFium `chromium/8125` via `pdfium-render` 0.9.5 · 20 fixture papers, 915 pages · full table: [out/report.md](out/report.md)

## Verdict: **pass** — PDFium is the Document engine's extractor. No Python bridge is needed for text.

| Criterion | Result | Status |
|---|---|---|
| Text equal or better than the current extractor (PyMuPDF, as ElectroChat calls it) | Word agreement median **0.95** (Latin-script papers 0.88–0.99). Where they differ, PDFium is better or equal on every defect measured, except right-to-left text (see finding 3) | **pass**, with one required fix |
| Character boxes present | Every character has an origin and a box (0 without) | **pass** |
| Text of 100 pages < 2 s | Median **0.36 s**, worst **1.93 s** (figure-heavy paper). Text plus every character position: median 0.43 s, worst 1.94 s | **pass**, tight on figure-heavy papers |
| PDFium boxes agree with the PDF.js text layer within 1 pt | Item **start**: 100% of matched items within 1 pt on 19/20 papers (Persian 97.6%); median error ≈ 0.006 pt. Item **end**: median 98.8% within 1 pt (min 94.2%) | **pass** |

### Defects, summed over the 20 papers

| Defect | PDFium | PyMuPDF | Note |
|---|---|---|---|
| Unexpanded ligatures (ﬁ, ﬂ, …) | **0** | 1,353 | PDFium expands them; better for search and quotes |
| Merged words (alphabetic runs ≥ 25 letters) | **5** | 7 | |
| Private-use glyphs | 3,078 | 3,078 | Identical: a property of the PDFs' fonts, not the extractor |
| Glyphs without a Unicode mapping | 107 marked `U+FFFD` | the same glyphs as raw control codes (`\x8a`, `\x0c`, …) | PDFium's marker is honest; PyMuPDF's control codes even broke page splitting (`\x0c`) |
| Letter-spaced headings ("C h a p t e r") | yes | yes | In the PDFs themselves |

## Findings the Document engine must handle

These go into `engines/document/DEVELOPING.md` and the conformance suite of `document.extract` (P1.M2.DOC.1–3).

1. **Surrogate pairs.** `FPDFText_GetUnicode` returns UTF-16 code units. Characters outside the BMP — every mathematical alphanumeric (𝐻, 𝑘, 𝜆, U+1D400–1D7FF) — arrive as two consecutive "chars" (high + low surrogate). `pdfium-render`'s `unicode_char()` returns `None` for each half. The adapter must read `unicode_value()` and pair them. The first spike version dropped them, and up to 28% of PDF.js items then had no PDFium match; after pairing, at most 6.4%. One fixture thesis contains 9.6 k such characters (a candidate thesis not kept in the set had 12.4 k).
2. **Soft-hyphen marker.** At a line-end hyphenation PDFium emits `\x02` ("propa\x02gating") where the visible text has "-". Normalise in the core: `\x02` + line break → join the word (and keep the original for exact quotes).
3. **Right-to-left text.** In the Persian paper some words come out with letters in visual (reversed) order ("هکبش" for "شبکه"), embedded English runs come out in reversed word order, and the zero-width non-joiner (U+200C) is lost. Word agreement with PyMuPDF: **0.41**. A prototype in the core style (keep PDFium's word breaks, re-order letters inside Arabic-script words by x position) raised it to **0.94**. Remaining: LTR runs inside RTL lines (Unicode bidi) and ZWNJ restoration from glyph gaps. Persian is a first-class language in the plan: this is required for Reader 1.0, task P1.M2.DOC.3 ("RTL runs handled"), with the Persian fixture in its conformance tests.
4. **Page loading dominates time.** Parsing a page's content stream is about 80–95% of extraction time in the slower papers; one figure-heavy page took 0.3–0.5 s by itself. Text extraction after loading is cheap. Consequences: load and extract the visible page first; preparation in the background (P1.M4.JOB); share one loaded page between rendering, text and objects; never load pages on the UI thread.
5. **Unmapped and private-use glyphs** (U+FFFD, U+E000–F8FF) are "unknown glyph" markers, not text. They are candidates for formula recognition or OCR of that region (P1.M4.EQ, P1.M4.OCR), and must never be quoted as evidence.
6. **Letter-spaced text** ("C h a p t e r") exists in real PDFs; word grouping in the core should merge single letters with uniform small gaps (both extractors fail here today).
7. **Reading order is not settled by this spike.** Two papers have in-order similarity 0.72–0.89 between the extractors with high word agreement — the same words in a different order. Without ground truth neither can be declared right; reading order comes from the layout model in P1.M4.OBJ.1.
8. **Residual geometry mismatches (≤ 6.4%)** remain only in math-heavy papers (composite symbols such as radicals and large operators). Conformance tolerance for `document.extract` geometry: start within 1 pt for ≥ 99% of non-math characters; math glyphs checked separately.

## Practical facts

- `pdfium.dll` 7.6 MB (3.9 MB compressed) — fits the < 80 MB installer budget easily.
- Open document: 10–40 ms. PDFium is not thread-safe: one owning thread, as planned (technical-structure.md §5).
- PyMuPDF is about as fast (median 0.33 s / 100 pages); speed is not a reason to keep it, and its AGPL licence is a reason not to.
- The NACA "scanned" report already carries a hidden OCR text layer. A truly image-only fixture must be derived from it (render pages to images) — added to P1.M2.DOC.8 / P1.M4.OCR.1.

## Not measured here

- Rendering speed and visual quality (spike S1 compares PDF.js display; PDFium rendering is measured with thumbnails in M2).
- Annotation writing (spike S4).
- Layout, tables, formulas (spike S5).
