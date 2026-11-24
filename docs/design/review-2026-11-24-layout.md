# Review: content spilling out of its box (founder report, 2026-11-24)

**Reported:** in Settings the controls, texts and buttons stood outside the dialog, over the page. The founder asked which document allowed it, how it is corrected, and that it cannot happen again in any part.

## Causes, and where each came from

| Cause in the code | Document that allowed it | Why nothing caught it |
|---|---|---|
| `.settings` asked for at least 560 px inside a dialog of 440 px (since 2026-11-15, P1.M4.MOD.1; the language-model section of 2026-11-24 made it taller) | Design language §5: spacing, hit targets and radii, but **no rule on who owns a width** | The style check looked only for raw colours and fonts |
| The kit's dialog had a fixed width and no height limit or scrolling | §8: a dialog is "one paragraph, max two buttons". **Settings, shortcuts and print had no component of their own**, so they used the dialog against its own rule | The design review looked at the main screens, in one window size; dialogs were not among them |
| Controls in neighbouring rows touched | §5.1 gives the grid, but no rule for rows of controls | Not measured |
| "Done when: e2e / installer" for the Settings task | Phase 1 plan, P1.M4.MOD.1: **no visual criterion** | The e2e checked that Settings works, not that it fits |
| Text at 150 % never tried | §13 promised 90–150 % "without breaking the frame" | No test changed the text size |

Looking for the same mistake everywhere (180 combinations of part, window and text size) found two more:

| Found | Document | Why |
|---|---|---|
| In windows up to 640 px high the tool rail's scroll bar took room from its buttons, which were cut off | §5.2 fixes the rail at 44 px; nothing on scroll bars | Only tall windows were tried |
| With both side panels open in a small window the paper was left about 50 px | §2 said the frame works from 1024 × 640 and that panels overlay the canvas below 1280 px; **the app allows 800 × 500, and overlaying was never built** | A written rule without a test |

## What changed
1. **Rules:** design language v1.4 — §5.4 *Boxes and their contents* (binding), §9.9 *Dialogs and sheets*, §8 *Sheet*, §2 one smallest window and 320 px for the paper, §13, §16 checklist.
2. **Programming guideline:** Part 3 (TypeScript), 5.4 and the review checklist (Part 7). **Phase 1 plan:** the design rule of every milestone names the layout check; P1.M4.MOD.1 has its visual criterion.
3. **Guards:** `e2e/layout.mjs` (every part, 9 sizes each) and the style check `ui/kit/scripts/widths.mjs` (no widths on content), part of `pnpm lint` and so of CI.
4. **Fixes:** dialog and sheet sizes with a scrolling body, rows 8 px apart, model rows that wrap, tool rail, room for the paper.

## Lesson
A rule without a check is a wish. Every binding layout rule now names the check that holds it.
