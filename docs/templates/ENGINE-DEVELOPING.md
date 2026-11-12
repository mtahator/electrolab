# Working on the <Engine> engine

## Purpose
One paragraph. Link to CONTRACT.md.

## Never change without a version bump
- Stable command inputs/outputs and guarantees listed in CONTRACT.md.
- Committed schemas in spec/.

## Rules
- Pure logic goes in core/ (no I/O, clock, randomness, network).
- I/O only in adapters/, behind ports.
- New behaviour = new command or new adapter, not changes to other engines.
- Depend only on other engines' *-contract crates, in the order of Draft 2.2 §6.3.
- Prompts, checklists, templates, thresholds go in assets/, not in code.

## How to check your work
- `cargo nextest run -p ep-<engine>-core -p ep-<engine>`
- `cargo test -p ep-<engine>-contract --test conformance`
- `cargo bench -p ep-<engine>` (budgets)
- `ep eval <engine> --golden` (AI quality, if applicable)
- `cargo xtask arch-check`

## Known pitfalls
- e.g. small models copy few-shot examples: use prefill and schemas instead.
- e.g. Windows paths and CRLF in fixtures.

## Decisions
- Links to relevant ADRs.
