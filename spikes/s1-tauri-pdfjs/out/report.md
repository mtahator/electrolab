## Open → first page (budget < 300 ms)

| paper | pages | cold canvas ms | warm canvas ms (median) | warm text layer ms (median) | warm max ms | bytes read | requests | word pick |
|---|---|---|---|---|---|---|---|---|
| arxiv-2106.03711v2 | 31 | 194.3 | 144.9 | 165.2 | 207.4 | 276 KB (3.0%) | 4 | 12/12 |
| arxiv-2111.07691v2 | 38 | 179 | 157.4 | 173.1 | 242.5 | 506 KB (29.4%) | 8 | 12/12 |
| arxiv-2406.19831v2 | 24 | 177.2 | 138.4 | 154.6 | 187.6 | 261 KB (3.8%) | 5 | 12/12 |
| fawiki-artificial-neural-network | 6 | 211.6 | 175.7 | 433.6 | 492.8 | 319 KB (71.4%) | 5 | 11/11 |
| naca-report-1135 | 70 | 229 | 192.9 | 198 | 241.4 | 4245 KB (64.2%) | 67 | 12/12 |
| plos-pone-0232683 | 28 | 257.4 | 203.7 | 243.6 | 269.8 | 837 KB (30.4%) | 10 | 12/12 |

## Continuous scroll (budget: frame < 16 ms → 60 fps)

| paper | speed px/s | pages | fps | frame ms median / p95 / p99 | frames > 21 ms | frames > 33 ms | frames showing a blank page | long tasks (max ms) | live canvases | JS heap MB |
|---|---|---|---|---|---|---|---|---|---|---|
| arxiv-2106.03711v2 | 1200 | 31 | 57.3 | 16.7 / 17 / 17.2 | 0.52% | 0.22% | 0% | 5 (600) | 8 | 6.1 |
| arxiv-2106.03711v2 | 3000 | 31 | 53.2 | 16.7 / 17 / 33.4 | 1.3% | 0.94% | 5.88% | 5 (594) | 8 | 12.8 |
| arxiv-2511.21915v1 | 1200 | 100 | 59.7 | 16.7 / 17 / 17.2 | 0.36% | 0.11% | 0% | 1 (61) | 10 | 36.9 |
| arxiv-2511.21915v1 | 3000 | 100 | 59.4 | 16.7 / 17 / 17.4 | 0.58% | 0.22% | 0% | 2 (62) | 10 | 39.4 |
| arxiv-2601.05155v2 | 1200 | 100 | 59.9 | 16.7 / 17 / 17.2 | 0.12% | 0.04% | 0% | 1 (60) | 10 | 8.9 |
| arxiv-2601.05155v2 | 3000 | 100 | 59.8 | 16.7 / 17 / 17.2 | 0.18% | 0.06% | 0% | 1 (50) | 10 | 9.9 |
| naca-report-1135 | 1200 | 70 | 58 | 16.7 / 17.1 / 33.4 | 3.12% | 0.96% | 0% | 3 (68) | 8 | 10.8 |
| naca-report-1135 | 3000 | 70 | 57.5 | 16.7 / 17.2 / 33.4 | 4.07% | 1.23% | 0% | 0 (0) | 8 | 10.6 |

## Jump to a page → rendered

| paper | jumps | median ms | p95 ms | max ms |
|---|---|---|---|---|
| arxiv-2511.21915v1 | 10 | 76.4 | 247.7 | 247.7 |
| naca-report-1135 | 10 | 54 | 659 | 659 |

## Real mouse input (genuine Windows mouse events, `real-input.ps1`)

| paper | word | double-click selected | trusted | word ok | line pieces selected | line ok |
|---|---|---|---|---|---|---|
| arxiv-2111.12066v2 | Gargya | Gargya | True | True | 1/1 | True |
| fawiki-artificial-neural-network | در | در | True | True | 71/73 | False |
| plos-pone-0232683 | nonlinear | nonlinear | True | True | 0/2 | False |

Display: 59.9 Hz, device pixel ratio 1.75, viewport [1265, 871], page scale 2 (fit width, capped at 2).
