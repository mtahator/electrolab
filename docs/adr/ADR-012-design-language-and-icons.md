# ADR-012: One design language for every app, one icon set (Lucide)

Status: **accepted**
Date: 2026-11-14
Plan reference: Draft 2.2 §28 (Reader), §12 (hosts and product compositions); Phase 1 milestone MD

## Context
The Reader works but its interface grew feature by feature: text buttons for every tool, a top bar that wraps onto two lines, ad-hoc spacing. More hosts are coming (Lab desktop in Phase 2, web consoles later) and the AI features of M5–M7 will add many panels. The founder asked for one modern, minimal, calm look shared by all apps, with icons for tools, and a reading space that stays clean after the AI features arrive.

## Decision
1. **`docs/design/design-language.md` is the binding design and UX guideline for every app**: principles (reading first, quiet until asked), the shared app frame (top bar, tool rail, left/right panels, canvas, status line), tokens, type, spacing, motion, components, patterns, the rules for annotations, notes and sticky notes, the rules for AI in the interface, writing, accessibility and a review checklist.
2. **`ui/kit` is the only implementation** of tokens, components, icons and strings; apps compose it and never fork it.
3. **Icons: Lucide** (ISC licence), bundled, imported per icon, wrapped in a kit `Icon` component; `sparkles` is reserved for AI actions.
4. A **design milestone (MD)** is added to Phase 1 before M4, so every later feature is built on the kit and the guideline.

## Alternatives considered
| Option | Why not |
|---|---|
| Keep text buttons, polish at the end (M8) | Restyling M4–M7's panels afterwards costs more than building them right; the founder uses the app daily now |
| Fluent UI icons / Windows look only | Ties the look to one platform; the product must look the same on macOS and Linux later |
| Material Symbols | Heavier, more decorative style; filled/outline mix invites inconsistency |
| A component library (e.g. a full design system package) | Large dependency for a small set of components; harder to keep the calm, document-first look; our kit stays small and owned |

## Consequences
- New UI must pass the checklist in the guideline; automated checks (tokens only, named icon buttons, contrast per theme) are added in MD.
- The JavaScript licence audit (P1.M8.SEC.2) includes Lucide.
- Changing a principle or the icon set needs a new ADR; other rules change by editing the guideline.
