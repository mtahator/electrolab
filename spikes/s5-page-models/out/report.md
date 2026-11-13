## Layout (9 fixture papers, first ≤ 20 pages each)

| model | threads | model ms/page (median of papers) | 20-page paper total s (min–max) | captions found | numbered equations found |
|---|---|---|---|---|---|
| egret | 4 | 364 | 7.24–8.59 | 86/96 | 296/299 |
| egret | 8 | 392 | 8.01–9.62 | 86/96 | 296/299 |
| heron-int8 | 4 | 8390 | 155.91–178.69 | 86/96 | 287/299 |
| heron-int8 | 8 | 6306 | 124.51–128.45 | 86/96 | 287/299 |
| heron | 8 | 599 | 12.24–13.72 | 86/96 | 295/299 |
| pp | 8 | 826 | 16.35–18.64 | 86/96 | 286/299 |

## OCR of image-only pages (rendered at 200 dpi, PDF text layer not used)

| engine | pages | per page s (detect + read) | word agreement with reference |
|---|---|---|---|
| PP-OCRv5 mobile, English (4 threads) | naca-report-1135.pdf p2–6 | 7.6, 7.3, 3.1, 4.1, 1.8 | 0.953, 0.93, 0.755, 0.714, 0.472 |
| PP-OCRv5 mobile, Arabic script, Persian page (lines reversed) | fawiki-artificial-neural-network.pdf p1–3 | 5.9, 6.1, 3.3 | 0.596, 0.631, 0.595 |
| Windows built-in OCR (Windows.Media.Ocr), English | naca-report-1135 p2–p6 | 0.24, 0.18, 0.14, 0.17, 0.14 | 0.933, 0.91, 0.705, 0.64, 0.317 |

## Formula → LaTeX (LaTeX-OCR / pix2tex ONNX via rapid_latex_ocr 0.0.9, CPU)

16 formulas from 2 papers; model load 17.05 s (first run includes download). Single-line: 9 formulas, 524–18287 ms (median 1097). Multi-line blocks: 7 formulas, 3985–30729 ms.
