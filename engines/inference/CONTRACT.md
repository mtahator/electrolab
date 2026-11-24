# Inference engine — contract

Version: 0.3.0      Stability: experimental      Tier: T1
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
| `inference.assign` | `{ task, model? }` → `Models` (the model for a task; none = the default again) | create |
| `inference.status` | `null` → `Status` | read |
| `inference.load` | `{ model? }` → `Status` (starts the runtime with that model, or the recommended one; returns when it answers) | read |
| `inference.unload` | `null` → `Status` (stops the runtime, frees the GPU) | read |

`LlmRequest { task, messages: [Message { role: system | user | assistant, content }], data?, schema?, prefill?, sensitivity: local_only | any, limits: { max_tokens, temperature }, background, stream?: { id } }`
- `background` answers (digests) wait for what a person asked, between answers.
- `task` names the job (`explain.draft`, `explain.check`, …); routing and model profiles key on it.
- `data` is the source text (paper passages). It is always sent as a separate, labelled data section after the instructions, **never** concatenated into a system or user instruction.
- `prefill` is the start of the answer; the model continues it, and the response includes it.
- `schema` (JSON Schema): the runtime is made to follow it (llama-server `json_schema`); the result is parsed and checked; on failure the call is retried once, then fails with `bad_output`.

`LlmResponse { content, model, route_reason, usage: { prompt_tokens, completion_tokens, first_token_ms, total_ms } }`

`Models { models: [ModelInfo], folders: [Folder], tasks: [Task], runtime }`
`ModelInfo { id, name, file, path, size, licence, kind: chat | embedding | vision_part | other, architecture, quantisation?, state: ready | loaded, recommended, context, problem? }`
`Folder { path, models, other_formats, missing, partial, own }` (`own`: added by the user, can be removed)
`Task { id: default | explain | check | sort | digest | find, model? }` (the user's choice; without one the suggestion answers)

### Automatic model search (founder request, 2026-11-24)
- Every model folder is searched **with its sub-folders** (at most 8 deep, 60 000 entries, 8 s; hidden and system folders are skipped, folder links are not followed). The folders: the app's `models/llm`, the ones the user added, `EP_LLM_DIR` (development).
- Every GGUF file is recognised **from its header, not its name**: what it is for (writes answers, search vectors, the picture part of another model, something else), name, size, quantisation, licence, context. Broken files are ignored; later parts of a split model are not models. Files in other formats are counted, so the app can say why they are not offered.
- **Suggestion:** the model measured for this kind of machine (ADR-005: Qwen3-4B Q4_K_M with a GPU, LFM2.5-1.2B Q8 without); else the largest chat model that runs well here (3 GB with a GPU, 1.5 GB without); else the smallest.
- **Tasks:** the user chooses a model for everything (`default`) and may choose another one per task: `explain` (writing the explanation) , `check` (checking it against the paper) and `sort` (suggesting folders for papers), `digest` (summaries) and `find` (asking the library). One model is loaded at a time, so two models for one answer means waiting for both.
- A chosen model that is no longer there falls back to the suggestion, and the route reason says so. A model that could not be started carries its `problem` until it starts.
- Choices are kept in a small file of the app; nothing is written into the model folders.

`Runtime { backend: cuda | vulkan | cpu | none, build }` — the llama.cpp build that will run (ADR-005: Vulkan by default, CUDA when present, CPU always; the Vulkan package also holds the CPU code, so it serves machines without a Vulkan driver, with no layers on the GPU).

`Status { state: stopped | starting | ready | failed, model?, backend?, error?, ready_ms? }`

Error kinds: `no_model` (no model file found), `no_runtime` (no llama.cpp build found), `start_failed`, `runtime_failed` (the sidecar stopped answering; it is restarted on the next call), `bad_output` (schema not met after one retry), `remote_not_allowed` (local-only data and no local backend), `cancelled`.

## Events (hub)
- `inference.token` `{ id, text }` — the next piece of a streamed answer (`stream.id` of the request). The final `LlmResponse` is the exact text.
- `model.loaded` `{ model, backend, ms }`, `model.unloaded` `{ model }`.
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
- 0.3.0 (2026-11-24): `LlmRequest.background` (people go first); tasks `digest` and `find`.
- 0.2.0 (2026-11-24): automatic model search, suggestion, a model per task; `search_models`, `add_folder`, `remove_folder`, `assign`.
- 0.1.0 (2026-11-21): first slice (P1.M5.INF.1–4, INF.6 local routing).
