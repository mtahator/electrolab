# ADR-015: Formula recognition — LaTeX-OCR on ONNX Runtime, optional download, in the Document engine

Status: **accepted**
Date: 2026-11-15
Amends: ADR-007 decision 3 (proposed → accepted, with the details below)
Plan reference: Draft 2.2 §14.2 (`FormulaModel` port), §28.3 (Equations); Phase 1 P1.M4.EQ.1–3; design note `docs/design/equations-figures-tables.md`

## Context
Equations in the Contents panel and popovers showed the PDF's math-font text, which is often unreadable (one paper's list showed a lone ","). The founder asked for every equation to be shown in full and correct, as LaTeX. Spike S5 measured LaTeX-OCR (pix2tex): single-line formulas looked correct with a median of 1.1 s on this laptop's CPU. Multi-line blocks took 4–31 s, and one very wide crop looped.

## Decision
1. **Model: LaTeX-OCR (pix2tex)**, the ONNX build published by RapidAI (release `v0.0.0`):
   - `image_resizer.onnx` (39 MB), `encoder.onnx` (89 MB), `decoder.onnx` (51 MB) and `tokenizer.json`;
   - pinned by SHA-256 in `engines/document/facade/src/models.rs`;
   - licences: weights MIT (LaTeX-OCR), conversion Apache-2.0 (RapidLaTeXOCR).
2. **Where:** behind the `FormulaModel` port of the **Document engine**, in the existing ONNX adapter (`ep-document-adapter-onnx`, `formula.rs`). No new engine: formulas are part of reading a document (§14.2).
3. **Our own pipeline in Rust**, matching the reference implementation (`rapid_latex_ocr` 0.0.9):
   - **Preprocessing:**
     - grey image, normalised so the ink is dark;
     - cropped to the ink's bounding box;
     - padded to multiples of 32;
     - fit within 672 × 192 (minimum 32 × 32).
   - **Resizer loop:** the resizer model predicts the best width in steps of 32; the image is rescaled until the width settles, at most 10 times.
   - **Encoding:** normalised with mean 0.7931 and std 0.1738, one channel.
   - **Decoding:** greedy (the reference uses temperature 10⁻⁵, which is greedy). The sequence starts at BOS = 1 and ends at EOS = 2. Logits come from the last position, with the full sequence and mask given each step.
   - **Text:** tokens joined, `Ġ` → space, special tokens removed, then the reference whitespace clean-up.
4. **Clean crops and limits:**
   - the equation number is cut off;
   - blank margins are trimmed;
   - blocks taller than about 1.8 lines are split into lines and joined as `\begin{aligned} … \\ … \end{aligned}`;
   - at most 512 tokens and 5 s per line;
   - a token repeated 12 times in a row ends the line (the S5 loop).
5. **Optional download (about 170 MB)** with resume and hash check, like the layout model:
   - offered in Settings → Page models, and the first time the user asks for LaTeX;
   - never in the installer (keeps it small), never automatic.
6. **When it runs:**
   - lazily, for an equation being shown or copied;
   - then in the background for all equations, after the other preparation stages, one at a time, at low priority;
   - results cached by file, equation id and model version.
7. **Without the model**, equations are shown as images of the formula (Contents, popovers), never as raw PDF text.

## Alternatives considered
| Option | Why not (now) |
|---|---|
| UniMERNet, PP-FormulaNet_plus (Apache-2.0) | Better on hard formulas, but no ready ONNX build; PP-FormulaNet_plus-M/L are large. To be re-measured on the golden set when an export exists |
| TexTeller (Apache-2.0, ONNX build exists) | About 300 M parameters, too slow per formula on CPU for this laptop; candidate for a GPU option |
| texify | Licence of the weights is not permissive enough for shipping |
| Nougat | Weights CC-BY-NC (see ADR-013) |
| Use the PDF's math-font text | Unreadable for many papers (the reported ","); kept only as cleaned search text |

## Later: a refinement loop with a small vision model (not built; founder direction 2026-11-18)
To make equations fully right without a large model (ADR-016 rejected a 24–48 GB page parser), the LaTeX is improved in a loop:
1. LaTeX-OCR reads the formula line (as now).
2. **Check:** the LaTeX is rendered (KaTeX or a TeX engine) and compared with the crop of the page (image similarity after normalising size and stroke).
3. **Correct:** if they differ, a **small vision-language model** (on-device class: a few GB at most, quantised, CPU or the 4 GB GPU) is given the crop, the rendering and the LaTeX, and returns a corrected LaTeX. Missing parts (a lost subscript, a wrong bracket, a dropped line) are completed.
4. **Stop:** when the rendering matches the crop, after a fixed number of rounds, or after a time limit. The best-matching version is kept.
5. **Measure first:** the loop is built only if it raises the golden equation set's score clearly over LaTeX-OCR alone, within the time budget (background stage, page in view first).

## Consequences
- Equations show as images at once, and as LaTeX once the model is installed: about 1 s per line, cached.
- Accuracy is that of pix2tex. A golden equation set (`fixtures/golden/equations/`) will measure it, and stronger models will be compared on it.
- The model registry gains four files. `document.models` lists "Equations as LaTeX" with its size and licence.
