# ADR-005: llama.cpp sidecar as the built-in model runtime; Qwen3-4B as the default local model

Status: **accepted** (runtime, sidecar mode, Vulkan-first) · **proposed** (default model — confirm on the golden Explain set, P0.C.2)
Date: 2026-11-12
Plan reference: Draft 2.2 §6.4 rule 8 and 12, §14.1, §17, §27 ("llama.cpp mode", "Default local model"); task P0.S3

## Context
ElectroChat 2.5 runs models with vLLM inside WSL: 60–90 s to start (77.6 s measured), NVIDIA only, Linux and Python required, not shippable in a small installer. The plan requires a small model ready in < 5 s, first token < 500 ms, guaranteed structured output, one inference contract for CUDA, Vulkan, Metal and CPU, and no WSL dependency. The user asked for a built-in runtime that runs quantised models such as Q4.

## Decision
1. **Runtime: llama.cpp `llama-server`, prebuilt, pinned in `natives.toml`, run as a sidecar process** by the Inference engine's `LlamaCppSidecar` adapter (behind `InferenceBackend`). Loopback only, random port, per-launch key, inside the app's Job Object.
2. **GPU builds: Vulkan by default (36 MB), CUDA as an optional download** for NVIDIA cards (157 MB + 404 MB runtime) when it is measurably faster on that machine; CPU build always present (23 MB). macOS (Metal) and Linux builds at the port stage.
3. **Models are GGUF files** at any quantisation (Q4_K_M by default for ≥ 3B models; Q8_0 for small models), downloaded on first use, pinned to a commit and verified by SHA-256 (as in `spikes/s3-llamacpp/models.json`).
4. **Default local model (proposed): Qwen3-4B-Instruct-2507 Q4_K_M** on GPUs with ≥ 4 GB; **LFM2.5-1.2B-Instruct Q8_0** on smaller GPUs and CPU-only machines. Explain on the test set: 98% vs 63% correct after the check (today's setup: 72%).
5. **Structured output always uses the server's JSON-Schema enforcement** (10/10 valid in every configuration).
6. **First load of a model runs a background warm-up** (one realistic request) to absorb Vulkan's one-time pause (9–17 s seen once per model).

## Alternatives considered
| Option | Why not |
|---|---|
| Keep vLLM (WSL) | 77.6 s start, NVIDIA + WSL only, large; violates §6.4 rule 8 |
| llama.cpp linked in-process | Needs compiling llama.cpp with the CUDA toolkit on every build; loses crash isolation; no speed gain to win |
| mistral.rs | Not needed: llama.cpp met every criterion; stays a possible second adapter |
| Keep LFM2.5-1.2B as default | Fast (checked Explain ~1.1 s) but wrong too often (63% after check); LFM licence limits commercial use above US$10 M revenue |
| CUDA as the only GPU build | ~560 MB extra download for no speed gain on this machine; NVIDIA only |

## Consequences
- The < 80 MB installer stays possible: CPU + Vulkan builds add ~60 MB uncompressed (to be measured compressed); models and CUDA are downloads.
- **Open: checked Explain takes ~2.8 s with Qwen 4B** against the < 2 s budget. P1.M5.UND.2 must close it (shorter drafts, passage search during the draft, or a mid-size model) and measure on the golden set; if it cannot, the budget is revisited with the founder.
- 4 GB GPUs hold one model at a time (Qwen 4B uses 3.1 GB); vision and text models swap.
- **Open: LFM Open License** for any LFM model we ship (enterprise customers above the revenue threshold) — founder / legal review before Reader 1.0 if LFM is kept as the small model.
- llama.cpp upgrades are pinned; S3's `bench.py` is the regression check.

## Evidence
`spikes/s3-llamacpp/RESULTS.md`, `spikes/s3-llamacpp/out/report.md`.
