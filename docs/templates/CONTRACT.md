# <Engine> engine — contract

Version: 0.1.0      Stability: experimental | stable | deprecated      Tier: T1
Owner: <name>

## Purpose
One sentence.

## Commands
### <engine>.<verb_noun>   (stability, tier)
- Input:  `TypeName { field: Type /* meaning, unit, default */ }`
- Output: `TypeName { ... }`   (streaming? progress events?)
- Errors: `ErrorEnum::Variant` — when, and what the caller should do
- Permission: read | create | execute | remote | external | destructive
- Example: input → output

## Events
- `<noun.past_tense> { fields }` — when it is published

## Uses (contracts only)
- `ep-<other>-contract` vX — for what

## Guarantees
- Invariants that always hold.

## Quality (AI-backed commands)
- Metric, golden set, threshold.

## Budget
- Typical input, time, memory.

## Assets
- Files in assets/ with their schemas and versions.

## Conformance
- tests/conformance/: cases and tolerances.

## Changelog and deprecations
