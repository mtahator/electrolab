## Speed and start-up

| backend | model | ready s (1st / 2nd start) | VRAM MB | first token ms (median / p95) | tokens/s | Explain draft s | check s | cases |
|---|---|---|---|---|---|---|---|---|
| cpu | LFM2.5-1.2B-Instruct-Q4_K_M.gguf | 0.79 / 0.78 | 0 | 1462 / 1989 | 37.6 | 2.34 | 3.31 | 15 |
| cpu | LFM2.5-1.2B-Instruct-Q8_0.gguf | 1.12 / 0.53 | 0 | 2985 / 3876 | 22.7 | 4.53 | 6.11 | 15 |
| cpu | Qwen3-4B-Instruct-2507-Q4_K_M.gguf | 3.34 / 2.55 | 0 | 4718 / 8521 | 10.5 | 12.44 | 8.05 | 15 |
| cuda | LFM2.5-1.2B-Instruct-Q4_K_M.gguf | 1.88 / 1.44 | 905 | 103 / 142 | 134.0 | 0.39 | 0.5 | 54 |
| cuda | LFM2.5-1.2B-Instruct-Q8_0.gguf | 3.2 / 1.96 | 1397 | 100 / 140 | 105.1 | 0.47 | 0.59 | 54 |
| cuda | Qwen3-4B-Instruct-2507-Q4_K_M.gguf | 3.29 / 2.55 | 3127 | 307 / 452 | 37.2 | 2.35 | 0.46 | 54 |
| vllm (WSL) | liquid-1.2b | 77.58 | — | 95 / 142 | 108.5 | 0.45 | 0.57 | 54 |
| vulkan | LFM2.5-1.2B-Instruct-Q4_K_M.gguf | 2.88 / 2.01 | 819 | 123 / 16951 | 137.8 | 0.39 | 0.5 | 15 |
| vulkan | LFM2.5-1.2B-Instruct-Q8_0.gguf | 7.69 / 2.28 | 1310 | 124 / 9480 | 104.1 | 0.5 | 0.59 | 15 |
| vulkan | Qwen3-4B-Instruct-2507-Q4_K_M.gguf | 4.75 / 3.74 | 3043 | 340 / 2066 | 41.3 | 2.12 | 0.48 | 15 |

## Explain quality (all 54 cases; same prompts as ElectroChat 2.5)

| backend | model | draft correct % | after check % | check said OK / corrected / unchecked | style problems % | JSON valid: schema / free |
|---|---|---|---|---|---|---|
| cuda | LFM2.5-1.2B-Instruct-Q4_K_M.gguf | 20.4 | 57.4 | 3 / 51 / 0 | 0.0 | 10/10 / 0/10 |
| cuda | LFM2.5-1.2B-Instruct-Q8_0.gguf | 38.9 | 63.0 | 14 / 40 / 0 | 1.9 | 10/10 / 2/10 |
| cuda | Qwen3-4B-Instruct-2507-Q4_K_M.gguf | 83.3 | 98.1 | 42 / 12 / 0 | 1.9 | 10/10 / 9/10 |
| vllm (WSL) | liquid-1.2b | 35.2 | 72.2 | 5 / 49 / 0 | 3.7 | 10/10 / 0/10 |

"Correct" = the explanation contains the paper's own long form of the abbreviation (≥ 80% of its words). "After check" = the text shown to the reader after ElectroChat's check step (draft if the check said OK, else the correction).
