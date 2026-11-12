# Spike P0.S3 — Results

Date: 2026-11-12 · llama.cpp b11988 · RTX 3050 Ti Laptop 4 GB, Ryzen 7 5800H, 16 GB · full tables: [out/report.md](out/report.md)

## Verdict: **pass** — llama.cpp replaces vLLM as the built-in runtime. Default model: **Qwen3-4B-Instruct-2507 Q4_K_M** on GPUs with ≥ 4 GB. One budget (checked Explain < 2 s) is missed by ~0.8 s and needs a follow-up.

| Criterion | Result | Status |
|---|---|---|
| Runs natively on Windows with CUDA, Vulkan and CPU | All three prebuilt builds run all three models; no WSL, no Python, no install | **pass** |
| Small model ready < 5 s | **1.4–3.3 s** on CUDA, 2.0–4.8 s on Vulkan, 0.5–3.3 s on CPU (second start; first start up to 7.7 s once, file not yet in the OS cache). **vLLM today: 77.6 s** | **pass** |
| First token (warm) < 500 ms | CUDA: 1.2B **100 ms**, 4B **307 ms** (p95 452). Vulkan: 123 / 340 ms. CPU: 1.5–4.7 s | **pass on GPU**; CPU only with a fast-first-token design (see 5) |
| Structured output | With the JSON Schema enforced by the server: **10/10 valid for every model and backend**. Asked for in the prompt only: 0–2/10 for LFM 1.2B, 9/10 for Qwen 4B | **pass** — always enforce schemas |
| Explain quality ≥ current setup | Current (vLLM, LFM2.5-1.2B FP8): 35% correct drafts, **72%** after the check. Same model on llama.cpp Q8: 39% → 63%. **Qwen3-4B Q4: 83% → 98%** | **pass by changing model**; same-model parity only for drafts |

## Explain in numbers (54 cases, CUDA, ElectroChat's exact prompts)

| Model | GPU memory | Draft correct | After check | First token | Draft done | Check | Checked answer |
|---|---|---|---|---|---|---|---|
| LFM2.5-1.2B FP8 on vLLM (today) | — | 35% | 72% | 95 ms | 0.45 s | 0.57 s | ~1.0 s |
| LFM2.5-1.2B Q8_0 | 1.4 GB | 39% | 63% | 100 ms | 0.47 s | 0.59 s | ~1.1 s |
| LFM2.5-1.2B Q4_K_M | 0.9 GB | 20% | 57% | 103 ms | 0.39 s | 0.50 s | ~0.9 s |
| **Qwen3-4B-Instruct-2507 Q4_K_M** | 3.1 GB | **83%** | **98%** | 307 ms | 2.35 s | 0.46 s | **~2.8 s** |

Typical errors of the 1.2B model: "PDE represents a mathematical tool used in optimization", "RANS refers to a modeling approach in fluid dynamics" — fluent and wrong. Its check step rarely says "OK": it rewrites most drafts and often ends the rewrite with "OK." (ElectroChat counts that as a correction). Qwen's check said "OK" to 42 of 54 drafts.

## Findings for the Inference engine (Phase 1, M5)

1. **Sidecar `llama-server` is the runtime** (already the plan's choice): prebuilt, one HTTP API for chat, streaming, prefill (a trailing assistant message is continued), JSON Schema and embeddings; crash isolation for free. In-process linking would need compiling llama.cpp with the CUDA toolkit on every build — not worth it.
2. **Vulkan as the default GPU build, CUDA as an optional pack.** Vulkan was as fast as CUDA here (104–138 tok/s small model, 41 tok/s Qwen) and is 36 MB against 157 MB + 404 MB CUDA runtime. It also covers AMD and Intel GPUs. CUDA is offered as a download when an NVIDIA GPU is found and Vulkan is unavailable or slower.
3. **Vulkan's one-time pause.** The first real request after loading a model paused **9.5 s and 17 s** (small model) and 2 s (Qwen); later requests and later process starts did not. Most likely the driver compiling and caching GPU programs (not proven: the driver cache was not cleared). Product: after the first load of a model (and after a driver update), run a realistic warm-up request in the background with a visible "preparing the model" note.
4. **Always enforce JSON Schemas** for structured output (the plan's guarantee "valid or fail after one retry" is cheap: grammar-constrained output was 10/10).
5. **CPU-only machines** get the small model and a different Explain flow: first token 1.5 s (1.2B Q4) is over budget; show the fast deterministic part first (definitions found in the paper by the check step's passage search) and stream the model text after.
6. **Quantisation matters for small models.** Q4 cost the 1.2B model half its correct drafts (39% → 20%). For the 4B model Q4_K_M is the sweet spot on 4 GB; Q8 does not fit with a useful context.
7. **4 GB is tight with Qwen 4B** (3.1 GB loaded, 4k context). Only one model at a time, as today; the vision model must swap with it.
8. **The checked-Explain budget (< 2 s) is missed by ~0.8 s with Qwen 4B.** The draft is the slow part (37 tok/s × ~70 tokens). Options for P1.M5.UND.2, to measure on the golden set: cap the draft at 2 sentences/60 tokens; run the passage search during the draft so the check starts at once; or a mid-size model (LFM2.5-2.6B Q4 1.6 GB, Qwen3-1.7B) — not tested here.
9. **This quality set is narrow** (abbreviations with a known long form). It shows the size of the gap, not the whole picture; the golden Explain set (P0.C.2: terms, sentences, equations, figures, tables) decides the final default.

## Licences

| Model | Licence | Consequence |
|---|---|---|
| Qwen3-4B-Instruct-2507 | Apache-2.0 | No restriction |
| LFM2.5 (all sizes) | LFM Open License 1.0 | Commercial use only while the licensee's annual revenue is under US$10 M — **open question** for the enterprise and industrial editions (founder / legal review) |

## Not measured here

- Memory of the whole app with a model loaded; long contexts (> 4k); embeddings and reranking (Inference engine M5.INF.7); vision models (`inference.vision_read`).
- mistral.rs (the plan's alternative): not needed, llama.cpp met every start and speed criterion.
