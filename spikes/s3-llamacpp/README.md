# Spike P0.S3 — llama.cpp on Windows: fast start, structured output, quality?

**Question (plans/phase-0, work package B):** can llama.cpp replace vLLM-in-WSL as the built-in model runtime on Windows — CUDA, Vulkan and CPU — with a small model ready in < 5 s, first token (warm) < 500 ms, valid JSON-schema output, and Explain quality at least as good as the current setup?
**Fall-back if not:** mistral.rs, or another model.

**Throw-away code.** Lessons go into ADR-005 and the Inference engine tasks of Phase 1 (M5).

## Set-up

| Part | Choice |
|---|---|
| Runtime | `llama-server` from llama.cpp release **b11988** (2026-10-29), prebuilt, pinned in `natives.toml`: CPU (23 MB), Vulkan (36 MB), CUDA 13.4 (157 MB + 404 MB CUDA runtime) |
| Models | `models.json` — pinned to a Hugging Face commit, SHA-256 verified, stored in `D:\local LLM\LLM-Models\gguf`: LFM2.5-1.2B-Instruct Q8_0 and Q4_K_M (the current ElectroChat model), Qwen3-4B-Instruct-2507 Q4_K_M (alternative) |
| Current setup (yardstick only) | ElectroChat's vLLM in WSL (`vllm-serve.sh liquid-1.2b`: LFM2.5-1.2B-Instruct, FP8), started exactly as the app starts it; nothing in ElectroChat changed |
| Hardware | RTX 3050 Ti Laptop (4 GB), Ryzen 7 5800H (8 cores), 16 GB RAM — the plan's minimum hardware |

## What is measured (`bench.py`)

- **Ready time:** process start → `/health` OK, two starts in a row (the second with the model file in the OS cache).
- **Explain, exactly as ElectroChat 2.5 does it** (`engine/pdf.js`): a draft (system prompt, the selected word with 500 characters before/after, the answer pre-filled with the word, temperature 0, ≤ 150 tokens), then a check against the paper's passages about the word (reply "OK" or a correction).
- **Cases (`cases.json`, from `gen_cases.py`):** 54 abbreviations from 15 fixture papers, each selected at a later *use* in the paper; the correct answer is the paper's own definition ("PINN" → "physics informed neural network"). Scored automatically: the explanation must contain ≥ 80% of the long form's words.
- **Structured output:** 10 extraction prompts with a method-graph-node JSON Schema, once enforced by the server (`response_format: json_schema`) and once only asked for in the prompt.
- **Speed:** first-token time, tokens per second, draft and check time; GPU memory with the model loaded.

## Reproduce

```
cargo xtask fetch-natives
cargo xtask fetch-fixtures            # and run spike S2 once: cases are built from its text output
pwsh spikes/s3-llamacpp/fetch-models.ps1
python -I -X utf8 spikes/s3-llamacpp/gen_cases.py spikes/s2-pdfium/out spikes/s3-llamacpp/cases.json
pwsh spikes/s3-llamacpp/run-all.ps1    # 9 llama.cpp runs
pwsh spikes/s3-llamacpp/run-vllm.ps1   # the current setup, once
python -I -X utf8 spikes/s3-llamacpp/summarise.py
```

Results: [RESULTS.md](RESULTS.md).
