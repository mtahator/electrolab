# ADR-004: SolidJS for the user interface; TypeScript types generated from the committed JSON Schemas

Status: **accepted** (founder's decision on the framework, 2026-11-13)
Date: 2026-11-13
Plan reference: Draft 2.2 §19.5, §21 (UI, tauri-specta), §27 ("UI framework"); technical-structure.md §7; Phase 1 M1.GEN.1, M1.HST.3

## Context
The Reader UI is a PDF viewer with many pages, several layers per page (canvas, text, annotations, overlays) and live updates (render queue, background preparation progress, streamed answers). The plan left React vs SolidJS open, and named tauri-specta for Rust → TypeScript types.

## Decision
1. **SolidJS + TypeScript (strict) + Vite** for `ui/kit`, `ui/reader` and later `ui/lab`. Fine-grained reactivity updates only what changed — no virtual-DOM diffing across hundreds of page layers — and the bundle stays small.
2. **TypeScript types are generated from the committed JSON Schemas** in `spec/schemas/` (produced from the Rust command definitions with `schemars`), not with tauri-specta. tauri-specta has no stable release for Tauri 2 (only release candidates in November 2026). The schemas are already the single source for the CLI, MCP tools, the language model's tools and the `schema-check` gate; generating TypeScript from them adds one consumer, not a second source of truth.
3. The generated file `ui/bindings/src/types.ts` is never edited by hand; CI fails if regenerating changes it.

## Alternatives considered
| Option | Why not |
|---|---|
| React | Larger ecosystem and hiring pool, but a viewer with many live layers needs memoisation discipline to stay fast; founder chose SolidJS |
| tauri-specta 2.0 release candidate | Pre-release dependency on the most central glue; schemas give the same result from a stable source |
| Hand-written TypeScript types | Drift from Rust — exactly what the plan forbids (§19.5) |

## Consequences
- Fewer ready-made component libraries; `ui/kit` holds our own small primitives (as planned).
- One generator step (`cargo xtask gen`) writes schemas, then TypeScript.
