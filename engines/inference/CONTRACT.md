# Inference engine — contract

Version: 0.4.0      Stability: experimental      Tier: T1
Crates: `ep-inference-contract` (types), `ep-inference-core` (ports, request building, Replay), `ep-inference-adapter-llamacpp` (sidecar), `ep-inference` (facade)

## Purpose
One interface to every language model, local or remote, with routing by policy and task and guaranteed structured output (Draft 2.2 §14.1, ADR-005). Phase 1 M5 first slice: the local llama.cpp sidecar and the Replay backend; remote adapters, embeddings and the separate `inference-host` process follow (P1.M5.INF.5–8).

## Commands
All experimental. Errors are `CommandError::Failed { kind }` with the kinds below.

| Command | Input → Output | Permission |
|---|---|---|
| `inference.complete` | `LlmRequest` → `LlmResponse` (streamed when `stream` is set) | read |
| `inference.models` | `null` → `Models { models: [ModelInfo], runtime: Runtime }` | read |
| `inference.search_models` | `null` → `Models` (searches the folders again) | read |
| `inference.add_folder` | `{ path }` → `Models` (the folder is searched at once, and from now on) | create |
| `inference.remove_folder` | `{ path }` → `Models` (nothing on disk is touched) | create |
| `inference.choose` | `{ order?, keep_ready? }` → `Models` (the order in which models are tried, `[]`: automatic again; whether one is kept ready) | create |
| `inference.status` | `null` → `Status` | read |
| `inference.load` | `{ model? }` → `Status` (starts the runtime with that model, or the first of the order that can be used; returns when it answers) | read |
| `inference.warm` | `null` → `Status` (makes a model ready when the user wants one kept; called when the app starts) | read |
| `inference.unload` | `null` → `Status` (stops the runtime, frees the GPU) | read |

`LlmRequest { task, messages: [Message { role: system | user | assistant, content }], data?, schema?, prefill?, sensitivity: local_only | any, limits: { max_tokens, temperature }, background, stream?: { id } }`
- `background` answers (digests) wait for what a person asked. One that is being written gives way at once and is begun again afterwards.
- `task` names the job (`explain.draft`, `explain.check`, …); routing and model profiles key on it.
- `data` is the source text (paper passages). It is always sent as a separate, labelled data section after the instructions, **never** concatenated into a system or user instruction.
- `prefill` is the start of the answer; the model continues it, and the response includes it.
- `schema` (JSON Schema): the runtime is made to follow it (llama-server `json_schema`); the result is parsed and checked; on failure the call is retried once — with twice the room when it was cut off, else a little less rigid — then fails with `bad_output`. The answer starts with the JSON at once: a model cannot "think" first and use up its room (founder report, 2026-11-24).

`LlmResponse { content, model, route_reason, usage: { prompt_tokens, completion_tokens, first_token_ms, total_ms } }`

`Models { models: [ModelInfo], folders: [Folder], order: [id], own_order, keep_ready, memory?: { device, total, free }, runtime }`
`ModelInfo { id, name, file, path, size, licence, kind: chat | embedding | vision_part | other, architecture, quantisation?, state: ready | loaded, recommended, context, needs, problem? }`
`Folder { path, models, other_formats, missing, partial, own }` (`own`: added by the user, can be removed)

### Automatic model search (founder request, 2026-11-24)
- Every model folder is searched **with its sub-folders** (at most 8 deep, 60 000 entries, 8 s; hidden and system folders are skipped, folder links are not followed). The folders: the app's `models/llm`, the ones the user added, `EP_LLM_DIR` (development).
- Every GGUF file is recognised **from its header, not its name**: what it is for (writes answers, search vectors, the picture part of another model, something else), name, size, quantisation, licence, context. Broken files are ignored; later parts of a split model are not models. Files in other formats are counted, so the app can say why they are not offered.
- **Suggestion:** the model measured for this kind of machine (ADR-005: Qwen3-4B Q4_K_M with a GPU, LFM2.5-1.2B Q8 without); else the largest chat model that runs well here (3 GB with a GPU, 1.5 GB without); else the smallest.
- Choices are kept in a small file of the app; nothing is written into the model folders.

### One model, kept ready (founder request, 2026-11-24)
- **One model answers every kind of work** and stays loaded, so that no answer waits for a model. It is loaded a moment after the app starts (`keep_ready`, can be switched off).
- **Order:** the user's first choice, second choice, …; without one: the suggestion, then smaller models (three in all). Models that are gone are left out.
- **Memory first:** before loading, the free memory of the graphics card is measured (`--list-devices` of the same build, less what other programs use — Windows promises every program nearly the whole card) and compared with what each model needs (weights, cache counted from the header, working memory). The first that fits is used, with a shorter context if that is what is missing. When none fits whole, the smallest is shared with the processor.
- **Next one** when a model does not start, or is slower than 3 tokens a second twice in a row. The reason is in `Status`, in the route reason and in `model.fallback`; the app says it once and offers the order.
- **Back to the first choice:** a background answer looks again after 10 minutes and changes back when the first choice fits in the place of the running one; a change of the order does so when nobody is waiting. A person's answer never waits for a change of model.

`Runtime { backend: cuda | vulkan | cpu | none, build }` — the llama.cpp build that will run (ADR-005: Vulkan by default, CUDA when present, CPU always; the Vulkan package also holds the CPU code, so it serves machines without a Vulkan driver, with no layers on the GPU).

`Status { state: stopped | starting | ready | failed, model?, backend?, error?, wanted?, fallback?, ready_ms? }`

Error kinds: `no_model` (no model file found), `no_runtime` (no llama.cpp build found), `start_failed`, `runtime_failed` (the sidecar stopped answering; it is restarted on the next call), `bad_output` (schema not met after one retry), `remote_not_allowed` (local-only data and no local backend), `cancelled`.

## Events (hub)
- `inference.token` `{ id, text }` — the next piece of a streamed answer (`stream.id` of the request). The final `LlmResponse` is the exact text.
- `model.loading` `{ model }`, `model.loaded` `{ model, backend, ms }`, `model.unloaded` `{ model }`.
- `model.fallback` `{ wanted, used, reason: not_enough_memory | start_failed | too_slow }` — another model than the first choice is used.
- `route.decided` `{ task, backend, model, reason }`.
- `inference.models_changed` `{}` — folders, choices or models changed.

## Ports
- `InferenceBackend` (core): `caps()`, `complete(request, on_token, cancel) -> Completion`.
- Adapters: `LlamaCppSidecar` (T1), `Replay` (tests: recorded replies keyed by task and request hash; unknown requests fail loudly).

## Sidecar rules (ADR-005, guideline "the sidecar is hostile-until-proven")
1. `llama-server` is started from the pinned build in `.natives/llama-cpp-*` (or next to the app), **bound to 127.0.0.1 on a free port**, with a per-launch random API key.
2. Ready when `/health` answers; at most 60 s; one background warm-up request after the first load of a model (Vulkan's one-time pause).
3. It is a child of the app and stopped with it (Job Object on Windows); if it stops answering, the next call restarts it once, then fails with `runtime_failed`.
4. Its output is parsed defensively: unknown fields ignored, broken lines skipped, a size limit per answer.
5. One model at a time (4 GB GPUs). Loading another model replaces the running one.
6. Started with an 8-bit cache (half the memory; without it when a model cannot), at most 512 MB for texts read before, on the real graphics card when there are two.
7. A server that writes nothing for 150 s is stuck: it is ended and started again once.

## Guarantees
1. With a schema, the output is valid JSON matching it, or the call fails with `bad_output` after one retry.
2. `data` is placed in its own section, labelled as data, after the instructions; it is never part of an instruction.
3. The route reason is returned with every answer and published as `route.decided`.
4. `local_only` requests never go to a remote backend (there is none in this slice; the router refuses them anyway).
5. Cancelling a call stops the generation (the HTTP stream is closed; llama-server stops the slot).

## Budget (ADR-005, spike S3)
- Small model ready < 5 s (S3: 1.4–3.3 s); first token (warm) < 500 ms on GPU (S3: 100–340 ms).

## Quality
- Conformance through the bus with the Replay backend (`engines/inference/facade/tests/conformance.rs`): streaming, prefill, schema retry and failure, cancel, local-only refusal, data section.
- A live test with the real sidecar runs when a llama.cpp build and a model are found (skipped otherwise).

## Changelog
- 0.4.0 (2026-11-24): one model in the user's order instead of one per task (`choose` replaces `assign`); memory measured before loading, next model when it does not fit, start or keep up; kept ready from the start of the app; background answers give way; schema answers never "think" first.
- 0.3.0 (2026-11-24): `LlmRequest.background` (people go first); tasks `digest` and `find`.
- 0.2.0 (2026-11-24): automatic model search, suggestion, a model per task; `search_models`, `add_folder`, `remove_folder`, `assign`.
- 0.1.0 (2026-11-21): first slice (P1.M5.INF.1–4, INF.6 local routing).
