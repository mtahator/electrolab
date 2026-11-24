# ADR-017: Unlimited-OCR Q4_K_M (Baidu) — measured, not used; PP-OCRv5 stays

Status: **rejected on measurement** (the founder's condition: use it if it is better, faster and more accurate on text)
Date: 2026-11-24
Relates to: ADR-013 (OCR), ADR-016 (OXR)

## Context
The founder asked to test Baidu's **Unlimited-OCR** in the Q4_K_M quantisation. It is a 3 B parameter vision-language model (MIT licence, June 2026), run here as the community GGUF build (1.95 GB model + 0.81 GB vision file) with the pinned llama.cpp b11988. Baidu publishes no GGUF itself.

## Measurement
English only (founder, 2026-11-21). Ten pages with a real text layer were drawn at 1700 px width and read by both; the score is the word-multiset F1 against the page's real text. PP-OCRv5 ran on the CPU through the engine (`ocr_bench` in `engines/document/facade/tests/intel.rs`); Unlimited-OCR on the RTX 3050 Ti (4 GB, 99 layers on the GPU, peak 3951 MiB), prompt "document parsing.", temperature 0, nothing else running. Its time includes loading the model for each page.

| Page | PP-OCRv5 F1 | s | Unlimited-OCR Q4_K_M F1 | s |
|---|---|---|---|---|
| arXiv 2111.12066, p. 2 | **0.999** | 12.9 | 0.938 | 53.2 |
| arXiv 2311.07440, p. 3 | **0.999** | 10.1 | 0.969 | 38.5 |
| arXiv 2410.21481, p. 2 | **0.994** | 5.2 | 0.884 | 45.8 |
| arXiv 2402.12652, p. 4 | **0.988** | 10.7 | 0.899 | 34.4 |
| arXiv 2207.11567, p. 3 | **0.984** | 9.8 | 0.661 | 40.1 |
| PLOS ONE 0232683, p. 3 | **0.855** | 5.8 | 0.550 | 107.9 |
| arXiv 2601.12341, p. 2 | **0.959** | 14.6 | 0.924 | 102.2 |
| Product manual, p. 1 | 1.000 | 1.3 | 1.000 | 55.8 |
| Slides, p. 2 | 1.000 | 0.8 | 1.000 | 52.7 |
| IRS form W-4, p. 1 | **0.973** | 9.4 | 0.754 | 140.4 |
| **Mean** | **0.975** | **8.1** | **0.858** | **67.1** |

Limits of the test: one quantisation, ten pages, words only. Unlimited-OCR writes formulas as LaTeX, which this score does not reward, and a warm server would save its loading time. An independent test also reports an accuracy cliff at Q4_K_M on dense pages; Q5_K_M and better were not asked for and do not fit a 4 GB GPU with the vision file.

## Decision
1. **PP-OCRv5 stays** (ADR-013): more accurate on 8 of 10 pages and equal on 2, about 8 times faster without a GPU, and it gives a box for every word, which search, selection and the text layer need. Unlimited-OCR gives boxes for blocks only.
2. **Not bundled, not offered.** It may be looked at again for formulas in the refinement loop (ADR-015, "Later").

## Consequences
- No change in the app. `ocr_bench` stays, to compare another OCR on the same pages.
