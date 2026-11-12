# ADR-001: Engines, not an app

Status: accepted
Date: 2026-11-12
Plan reference: Draft 2.2 §6–§9, §12; task P0.D

## Context
The product must serve several hosts (Reader, Lab desktop, CLI, MCP, later Python and a server) and grow from T1 to T3 without rewrites. It is built by a small team, so the code must stay easy to change in small, well-bounded, tested units.

## Decision
The system is a set of twelve independent engines on a shared foundation. Each engine is defined by a contract (commands, events, guarantees, errors, quality, budget, stability, version) and has the same internal shape: `contract/ core/ ports/ adapters/ facade/ assets/ cli/ tests/ benches/`. Engines use each other only through contract crates, in the fixed order of §6.3. Hosts only wire engines together. Contracts start `experimental` and become `stable` only after real use; stable contracts change only additively within a major version.

## Alternatives considered
| Option | Why not |
|---|---|
| One desktop application with internal modules | Logic would be written per surface (UI, CLI, MCP); hard to reuse in Python or a server |
| Microservices from day one | Operational weight with no users; the bus with in-process transport gives the same boundaries for free |

## Consequences
- Every capability is a command, reachable from every host; surfaces are generated, not written.
- Boundaries are enforced by `cargo xtask arch-check`, schema checks and conformance suites in CI.
- More crates and some ceremony per feature; kept small by "folders created when first needed".
