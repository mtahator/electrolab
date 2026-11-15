# Equations, figures and tables in the Contents panel — problem, causes, fixes

Version 1.0 · 2026-11-15 · Phase 1, M4 (tasks P1.M4.EQ.1–5, P1.M4.OBJ.6) · Decisions: [ADR-007](../adr/ADR-007-page-understanding-models.md), [ADR-015](../adr/ADR-015-formula-recognition.md)

## 1. What the founder saw

- The **Equations** list in the Contents panel showed entries such as a lone **","** instead of the equation.
- The founder asked for each equation to be shown in full and correct, as LaTeX.
- Figures and tables were listed by caption text only. They should show the figure or table itself.

## 2. Causes (measured on the fixture papers)

| # | Cause | Where | Effect |
|---|---|---|---|
| 1 | **Equation text comes from the PDF's math fonts.** PDFium returns the glyphs in drawing order: subscripts flattened ("xi+1"), big brackets as private-use glyphs (U+F8EE…), control characters (U+0010/U+0011) for delimiters. | `document.objects` → `text` of an equation | Unreadable text in the list; for some papers almost nothing but punctuation |
| 2 | **Without the layout model, an equation is any line ending in "(n)".** The object covers that one line only. When the number is set apart from its formula (a line of its own, or after a comma), the line holds only ", (3)". | `core/intel/layout.rs`, text-only fallback | Removing the label leaves **","**; the formula body above is missing; prose ending in "(3)" can be taken for an equation |
| 3 | **The Contents list shows text for every object.** | `ui/reader/src/shell/SidePanel.tsx` (`Jump`) | Equations, figures and tables are never shown as they look on the page |
| 4 | **No formula model yet.** `document.equation` returns `latex: null` with a note. | `facade/src/intel.rs` | No LaTeX anywhere |

With the layout model, the equations themselves are found well: 13 of 13 numbered equations on `arxiv-2111.12066v2`; spike S5: 296 of 299. Only their text is unusable (cause 1).

## 3. Fixes

### 3.1 Show objects as they look (cause 3) — P1.M4.OBJ.6

Every entry in the Figures, Tables and Equations lists shows a **picture of the object**:
- it is cut from the page by `document.region` with the object's rectangle;
- it is drawn by the engine, cached, and follows the paper colour, so dark papers have dark crops;
- it is loaded only when the list is open and the entry scrolls into view.

Entries look like this:
- **Figures and tables:** a thumbnail (up to 2 lines tall), then the label ("Figure 3") and the caption's first words. A click goes to the object; hovering shows it larger, as the in-text popover does.
- **Equations:** the formula image at reading size, with its number. When LaTeX is known, it is the copy text and the tooltip. Raw PDF text is **never** shown for an equation.

### 3.2 Clean text where text is still used (cause 1) — P1.M4.EQ.4

The text of an equation, used by search snippets and as the fallback copy text, is cleaned:
- private-use glyphs and control characters are dropped;
- whitespace is collapsed.

A text that is left with no letters or digits counts as **no text**. It is never shown as a ","-like entry.

### 3.3 Find equations without the layout model properly (cause 2) — P1.M4.EQ.5

The text-only rule changes from "a line ending in (n)" to "**a displayed formula block with a number**":
1. **Number line.** The line ends in "(n)".
   - The text before the number must not read like prose: no run of four or more ordinary words that ends in a full stop, and not "see (3)" / "in (3)".
   - The line must be set apart from the column's text: indented on both sides (centred), or the number separated by a wide gap.
2. **The block.** Lines directly above the number line that are also displayed (centred, close together, not prose) belong to the same formula. The equation's rectangle is the union of the block, without the number.
3. **The number alone.** A number line with nothing else (", (3)" or "(3)") takes the displayed lines beside or above it as its formula. If there are none, it is not an equation.

The layout model stays the first choice. These rules only fill in for papers prepared without it.

### 3.4 LaTeX for every equation (cause 4) — P1.M4.EQ.1–3, [ADR-015](../adr/ADR-015-formula-recognition.md)

- **Model.** A formula recogniser behind the existing `FormulaModel` port: LaTeX-OCR (pix2tex; MIT weights, ONNX build by RapidAI, Apache-2.0). It runs on ONNX Runtime like the other page models, inside the **Document engine** (an adapter, not a new engine).
- **Pipeline.** Preprocessing, the resizer loop, greedy decoding and tokenizer decoding are re-implemented in Rust. They match the reference implementation step by step (§4).
- **Clean crops** (spike S5):
  - the equation number is cut off (PDFium knows where it is);
  - blank space is trimmed;
  - blocks taller than one line are split into lines and converted line by line, then joined with `\\` in an `aligned` environment;
  - output is capped at 512 tokens and 5 s per line; a repeated-token loop stops early.
- **When it runs.**
  - Lazily, when an equation is shown (popover, Contents list, `document.equation`), then cached by file, equation and model version.
  - In the background after the other preparation stages, for every equation, at low priority.
- **Install.** It is optional (about 170 MB): offered in Settings → Page models and on first use ("Show equations as LaTeX"), like the layout model. Without it, equations are shown as images (§3.1).

## 4. How it is checked

| Check | Where |
|---|---|
| No equation entry without a picture; no entry whose text is only punctuation | `tests/intel.rs` (`equations_are_whole`) |
| Text-only rules: every numbered display equation of `arxiv-2111.12066v2` found as a block containing its formula; no prose line found | same test, without the layout model |
| LaTeX matches the reference implementation on the S5 formula crops (same tokens, greedy) | `adapters/onnx` test with `EP_MODELS_DIR` |
| LaTeX quality: single-line equations of the fixture papers compile and are close to the printed formula (normalised edit distance on a golden set) | `fixtures/golden/equations/` (P1.M4.EQ.1) |
| Contents lists show pictures for figures, tables and equations; a click goes to the object | `e2e/intel.mjs` |

## 5. Not in this change

- LaTeX for inline math inside sentences: text search already finds `C_p` and similar notation (`document.search`).
- Stronger formula models (UniMERNet, PP-FormulaNet_plus, TexTeller): they have no ready ONNX build or are heavier. To be re-measured against the golden set later (ADR-015).
