# ElectroPaper design language and UX guideline

Version 1.6 · 2026-11-24 · applies to **every ElectroPaper app**: the Reader, the Lab desktop app, later the web and server consoles, and any panel an engine contributes. Owner: `ui/kit`. Decision: [ADR-012](../adr/ADR-012-design-language-and-icons.md).

> **In one sentence:** the paper is the hero; the app is a quiet, well-made desk around it that shows tools when you reach for them and puts them away when you don't.

This document is binding. A screen that breaks a rule here is a bug, unless the rule is changed first (see §15 Governance). Rules are written so they can be checked: "must" is required, "should" is the default and needs a reason to break, "may" is allowed.

---

## Contents
1. [Principles](#1-principles)
2. [The app frame](#2-the-app-frame)
3. [Colour](#3-colour)
4. [Typography](#4-typography)
5. [Space, size and shape](#5-space-size-and-shape)
6. [Icons](#6-icons)
7. [Motion](#7-motion)
8. [Components](#8-components)
9. [Interaction patterns](#9-interaction-patterns)
10. [Annotations, notes and sticky notes](#10-annotations-notes-and-sticky-notes)
11. [AI in the interface](#11-ai-in-the-interface)
12. [Writing for the interface](#12-writing-for-the-interface)
13. [Accessibility](#13-accessibility)
14. [One family of apps](#14-one-family-of-apps)
15. [Governance and review](#15-governance-and-review)
16. [Review checklist](#16-review-checklist)

---

## 1. Principles

These decide every case the rest of this document does not cover. When two conflict, the earlier one wins.

1. **Reading first.** The document gets the space, the contrast and the calm. Nothing moves, blinks or pops up over the text unless the user asked for it. Chrome (bars, panels, badges) is quieter than the page: lower contrast, smaller type, fewer colours.
2. **Quiet until asked.** Tools appear where the user's attention already is (the selection bar over selected text, a card next to a mark) and leave when the task is done. Nothing opens by itself, nothing nags, nothing celebrates.
3. **Clear over clever.** A user who has never seen the app understands each screen in a few seconds. Every icon has a name on hover; every action says what it will do; every state (saving, working, offline) is visible in one consistent place.
4. **One way, everywhere.** The same thing looks and behaves the same in every app and every panel: one button style, one card, one menu, one way to undo. A new feature reuses a pattern before it invents one.
5. **Never lose work, rarely ask.** Changes are saved at once and can be undone; so the app almost never needs "Are you sure?". Confirmation is kept for actions that cannot be undone.
6. **Honest and traceable.** Anything the app concludes (an explanation, an answer, a summary) shows where it came from, with a click to that place in the paper. Uncertainty is stated, not hidden.
7. **Fast feels calm.** Respond within 100 ms to every input; when something takes longer, show progress in place, never block reading.
8. **Everyone can use it.** Keyboard only, screen readers, high contrast, large text: built in from the first version of each screen (§13).

---

## 2. The app frame

Every app uses the same frame. Regions may be empty, never rearranged.

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ Top bar (40 px): library · panels · add ─── back · page / zoom ─── find · notes · ⋯ │
├──┬──────────────┬───────────────────────────────────────────┬──────────────────┤
│T │ Left panel   │                                           │ Right panel      │
│o │ (navigation) │            Canvas (the document)          │ (work on it)     │
│o │ Contents     │                                           │ Notes            │
│l │ Pages        │       ┌───────────────────────────┐       │ Card of a mark   │
│  │ Find         │       │                           │       │ Assistant (AI)   │
│r │ Bookmarks    │       │          page             │  ◦ ◦  │                  │
│a │ Annotations  │       │                           │ margin│                  │
│i │              │       └───────────────────────────┘ notes │                  │
│l │              │                                           │                  │
├──┴──────────────┴───────────────────────────────────────────┴──────────────────┤
│ Status line (24 px): saved · background work · model state · page n of m        │
└────────────────────────────────────────────────────────────────────────────────┘
```

| Region | Holds | Rules |
|---|---|---|
| **Top bar** | Library, panel switches, add PDF, back/forward, page and zoom, find, split, notes, theme, command palette, overflow menu `⋯` | One row, 40 px. Never wraps: items that do not fit go into `⋯`. No more than two text buttons; the rest are icons. Document tabs sit at the top of each pane, so split view keeps one tab strip per pane. |
| **Tool rail** | Annotation and page tools (Reader); mode tools (Lab) | Vertical, 44 px wide, icons only (20 px) with tooltips and shortcuts. Grouped with 8 px gaps, no labels. The active tool has a filled background in the accent tint. Tool options (colour, width, category) appear in a small flyout next to the rail when a tool is chosen, not as a permanent bar. |
| **Left panel** | Navigation: contents, page thumbnails, find results, bookmarks, the list of annotations | 240–480 px, resizable, remembers its width. Chosen with a vertical icon strip at its edge. Closes with the same icon or `Ctrl+B`. |
| **Canvas** | The document | Neutral surround (`--viewer-bg`). Pages have a soft shadow and no border. Nothing is drawn on the canvas except the document, its annotations, the selection bar and cards the user opened. |
| **Right panel** | Work on the document: the paper's notes, the card of the selected mark, the assistant | Closed by default. Opens when the user asks (a click, a shortcut, or "Open in notes" from a card). 320–560 px. One right panel at a time; it switches content with tabs at its top. |
| **Status line** | Saved state, background work, model state, page position | 24 px, small muted text, no colour except for problems. The only place for "working…" messages that are not tied to a spot on the page. |

**Focus mode** (`F11` or `Ctrl+.`): hides everything except the canvas and a thin page indicator; moving the pointer to the top edge shows the top bar for that moment. Every app with a canvas offers it.

**Density.** Two densities: *comfortable* (default) and *compact* (−4 px row height, for small screens). No other variation.

**Window sizes.** The frame works from **800 × 500** up: the smallest window the app allows (`tauri.conf.json`), so there is one number, and it is tested. The paper always keeps at least **320 px**: side panels get narrower, and when both do not fit, opening one closes the other. (Until 2026-11-24 this said 1024 × 640 and "panels overlay the canvas below 1280 px"; neither was built or tested.)

---

## 3. Colour

### 3.1 Neutrals do the work
The interface is built from neutrals; colour carries meaning. Every colour is a token in `ui/kit/src/tokens.css`; components never use raw colour values.

| Token | Role |
|---|---|
| `--bg` | Window background, cards, menus |
| `--bg-panel` | Panels, bars |
| `--bg-sunken` | Hover, wells, inputs on panels |
| `--fg` | Main text |
| `--fg-muted` | Secondary text, icons at rest |
| `--border` | Hairlines (1 px) |
| `--accent` / `--accent-fg` | The one interactive colour: focus ring, primary button, active tool, links in the UI |
| `--danger` | Destructive actions and errors only |
| `--success`, `--warning` | Status only (status line, validation results), never decoration |
| `--viewer-bg`, `--page-bg`, `--page-ink`, `--page-shadow` | The canvas and recoloured pages |

Rules:
- **One accent per screen.** It marks what is active or primary; at most one primary button per view.
- **Colour never carries meaning alone**: errors also have an icon and text; categories also have a name.
- **Contrast:** text ≥ 4.5 : 1, large text and icons ≥ 3 : 1, in every theme (checked in CI by the theme test, §15).
- **No gradients, no glow, no coloured shadows.** Shadows are neutral and only for things that float (§5.3).

### 3.2 Themes and paper
Two separate choices, both in the theme menu (`sun-moon`):

- **App theme** — the frame: **Light** (default), **Sepia**, **Dark**, **Night** (warm, dim for low light), **High contrast**. A new screen is checked in all five before it is merged.
- **Paper** — how pages look: **Match the app** (default), **Original colours** (the PDF exactly as printed), **Sepia** (warm paper, ink keeps its colours), **Dark**, **Night**, **High contrast** (text and lines redrawn; figures keep their own colours).

So a dark app can show pages in their true colours, and a light app can show dark pages. Paper colours are tokens set by `data-paper` (`--page-bg`, `--page-ink`); nothing on the page reads the app theme.

### 3.3 Annotation colours
Annotation colours are a separate palette, chosen to read well over text on both white and recoloured pages and to stay distinct for common colour-vision deficiencies:

| Name | Value | Default meaning |
|---|---|---|
| Yellow | `#ffd400` | General highlight |
| Green | `#7bd88f` | Agree, result |
| Blue | `#74c0fc` | Definition, method |
| Red | `#ff8787` | Problem, to check |
| Purple | `#d0a2f7` | Question |

Drawing colours (ink, shapes, text on the page): red `#e03131`, blue `#1971c2`, green `#2f9e44`, orange `#f08c00`, graphite `#212529`. Users may not pick arbitrary colours in Phase 1 — a small fixed palette keeps papers readable and filters useful.

---

## 4. Typography

| Use | Font | Size / line height | Weight |
|---|---|---|---|
| UI text | Segoe UI Variable (Windows), system UI elsewhere | 13 / 18 px | 400 |
| UI labels, menus | same | 13 / 18 | 400; 600 for the current item |
| Small text (status, meta) | same | 12 / 16 | 400 |
| Panel titles | same | 14 / 20 | 600 |
| Dialog titles | same | 16 / 22 | 600 |
| Note and answer text (reading in panels) | same, or the user's choice of serif (Georgia / Charter) | 14 / 22, max 72 characters per line | 400 |
| Code, numbers in tables | Cascadia Mono / Consolas | 12 / 18 | 400 |

Rules: sentence case everywhere (no Title Case, no ALL CAPS); at most three sizes on one screen; numbers that line up use tabular figures; no italics in UI except quotes from the paper; the user's text size setting scales every size above.

---

## 5. Space, size and shape

### 5.1 Space
A 4 px grid: tokens `--space-1` (4) `--space-2` (8) `--space-3` (12) `--space-4` (16) `--space-6` (24). Inside a component use 4–12; between groups use 16–24. Align to a grid; never nudge by single pixels.

### 5.2 Sizes
| Element | Size |
|---|---|
| Hit target (anything clickable) | ≥ 28 × 28 px (compact), ≥ 32 × 32 px (comfortable) |
| Icon button | 32 × 32, icon 20 |
| Text button | 32 px tall, 12 px side padding |
| Input, select | 32 px tall |
| Row in a list | 32 px (one line), 52 px (two lines) |
| Tool rail | 44 px wide |

### 5.3 Shape and depth
- Radius: `--radius` 6 px for controls, 10 px for cards and popovers, 50 % for swatches and avatars.
- Depth has three levels and is used only for what floats above the page:
  - **0 flat** — bars, panels (separated by 1 px `--border`);
  - **1 raised** — cards next to a mark, the selection bar: `0 4px 16px rgb(0 0 0 / 14%)`;
  - **2 overlay** — menus, the command palette, dialogs: `0 8px 32px rgb(0 0 0 / 22%)`.
- Prefer a hairline to a shadow, and space to a hairline.

### 5.4 Boxes and their contents (binding, added 2026-11-24)
1. **The container owns the width; its content never does.** Fixed and minimum widths belong to surfaces of the kit (dialog, sheet, menu, card, panel, palette). Content inside fills the width it is given (`flex`, `min-width: 0`) and wraps. No `min-width`, and no `vw`, on content.
2. **Every surface has a limit in both directions** and says what happens at it: wrap, shorten with an ellipsis and a tooltip, or scroll. Nothing spills, nothing is cut off without one of the three.
3. **A scroll bar takes its own room.** A strip too narrow for one (the tool rail) hides it and still scrolls.
4. **Sizes to hold:** 800 × 500, 1024 × 640 and a large window, each at 90, 100 and 150 % text, with the longest text the app has.
5. **Checked by machine, not by eye:** `e2e/layout.mjs` opens every part of the interface at all of these sizes; the style check refuses widths outside surfaces. A new part is added to `e2e/layout.mjs` in the same change.

---

## 6. Icons

**One icon set: [Lucide](https://lucide.dev)** (ISC licence), 24 px grid, 1.75 px stroke, round caps — a quiet, consistent line style that suits a reading tool. Bundled with the app (no network), imported per icon so only used icons ship. Icons are wrapped in `ui/kit` (`<Icon name=… />`) so the set can be swapped in one place.

Rules:
- **Use icons for tools and frequent actions** (tool rail, top bar, card headers, list rows). Use text for actions that need words to be understood (Save into PDF, Reply, Cancel) and in menus (icon optional, on the left).
- **Every icon-only control has a tooltip with its name and shortcut** (`Highlight (H)`) and an accessible name.
- Sizes: 20 px in bars and the rail, 16 px in menus, lists and cards, 14 px inline with small text. Colour `--fg-muted` at rest, `--fg` on hover, `--accent` when active.
- **One meaning per icon across all apps.** New meanings get new icons; no icon is reused for a different action.
- No emoji in the interface; no filled and outline mix within one group; no custom-drawn icons unless Lucide has nothing suitable (then drawn on the same grid and stroke, kept in `ui/kit/icons/`).

### 6.1 Icon map (Reader)

| Action | Lucide icon | Shortcut |
|---|---|---|
| Select | `mouse-pointer-2` | V |
| Highlight | `highlighter` | H |
| Underline | `underline` | U |
| Strike out | `strikethrough` | S |
| Squiggly | `spline` | — (selection bar, rail) |
| Sticky note (comment pin) | `message-circle-plus` | N |
| Text on page | `type` | T |
| Draw | `pen-line` | D |
| Rectangle | `square` | R |
| Ellipse | `circle` | O |
| Line | `minus` | L |
| Arrow | `move-up-right` | A |
| Eraser | `eraser` | E |
| Undo / redo | `undo-2` / `redo-2` | Ctrl+Z / Ctrl+Y |
| Save into PDF | `file-down` | Ctrl+S |
| Comment | `message-circle-plus` | — |
| Notes in the margin | `messages-square` | Ctrl+Shift+M |
| Copy | `copy` | Ctrl+C |
| Cite | `quote` | Ctrl+Shift+C |
| Explain (AI) | `sparkles` | double-click a term |
| Ask the paper (AI) | `message-circle-question-mark` | Ctrl+J |
| Notes panel | `notebook-pen` | Ctrl+Shift+N |
| Contents | `list-tree` | — |
| Pages | `layout-grid` | — |
| Find | `search` | Ctrl+F |
| Bookmarks | `bookmark` | Ctrl+D (add) |
| Annotations list | `list-checks` | — |
| Library | `library` | Ctrl+L |
| Add PDF | `file-plus` | Ctrl+O |
| Open URL | `link` | Ctrl+Shift+O |
| Split view | `columns-2` | Ctrl+\ |
| Back / forward | `arrow-left` / `arrow-right` | Alt+← / Alt+→ |
| Zoom out / in | `zoom-out` / `zoom-in` | Ctrl+− / Ctrl+= |
| Theme | `sun-moon` | — |
| Focus mode | `maximize-2` | F11, Ctrl+. |
| More | `ellipsis` | — |
| Close | `x` | Ctrl+W (tab), Esc (card) |
| Delete | `trash` | Delete |
| Earlier versions | `rotate-ccw-clock` | — |
| Hide / show annotations | `eye-off` / `eye` | Ctrl+H |
| Side panel / notes panel | `panel-left` / `notebook-pen` | Ctrl+B / Ctrl+Shift+N |
| Command palette | `command` | Ctrl+K |
| Resolved | `circle-check` | — |
| Settings | `settings` | Ctrl+, |

`sparkles` is used **only** for AI actions, so users always know when a result comes from a model.

---

## 7. Motion

- Motion explains a change of place (a panel opening, a card appearing next to its mark); it never decorates.
- Durations: 120 ms for small things (hover, press, tooltips after a 500 ms delay), 180 ms for panels and cards, never above 240 ms. Easing: `cubic-bezier(0.2, 0, 0, 1)` in, linear for progress.
- **Motion follows the hand** (amended 2026-11-15, founder request; review R8):
  - pages that are still loading show a preview that blurs **only along the scroll direction**, and **only while the reader flies through several pages** (fast scrolling, more than about a page per 0.4 s, over more than a page). Small or slow movements never blur: the reader must be able to read what moves. At rest nothing blurs (amended 2026-11-15 after founder feedback);
  - a sharp page fades in over its preview;
  - a jump within about two screens glides; longer jumps are instant;
  - a zoom gesture scales what is shown at once, and the pages are redrawn sharp when the gesture ends.
- No page-turn effects, no bounces.
- No looping animation except a progress indicator while something is actually running.
- `prefers-reduced-motion`: all transitions become instant.

---

## 8. Components

All components live in `ui/kit` and are the only way to build UI. Each has: states (rest, hover, pressed, focus, disabled, busy), an accessible name, and a story in the kit's gallery page.

| Component | Use | Notes |
|---|---|---|
| **Button** | Text actions | Variants: *primary* (one per view), *secondary* (default), *quiet* (no border, for toolbars), *danger*. Verb labels. |
| **IconButton** | Tools, frequent actions | 32 × 32, tooltip required, `aria-pressed` for toggles. |
| **ToolRail / ToolGroup** | Tools | Vertical groups; active state; options flyout. |
| **Segmented** | 2–4 exclusive choices (layout, filter) | Icons or short words, not both. |
| **Swatch row** | Colour choice | Fixed palettes (§3.3); checked state with a ring. |
| **Menu / ContextMenu** | Secondary actions | Grouped with separators; shortcuts right-aligned in muted text; max 12 items, else split. |
| **Popover** | Small tool UI next to its anchor (selection bar, tool options) | Level-1 depth; closes on Esc and outside click; never covers the anchor. |
| **Card** | A thing on the page (annotation, sticky note, explanation) | Header (icon, title, meta, close), body, footer actions. 300–360 px wide. |
| **Panel** | Left/right panels | Title row with icon tabs, scrollable body, optional footer. |
| **Tabs** | Documents (top bar), panel views | Document tabs: title truncated in the middle, close on hover. |
| **List / ListRow** | Libraries, annotation lists, results | One- or two-line rows, colour bar for annotation colour, keyboard navigation. |
| **Input, TextArea, Select, Checkbox, Search** | Forms and filters | Labels always visible (placeholders are hints, not labels). |
| **Dialog** | Only when the app must stop and ask (irreversible actions, first run) | Title, one paragraph, max two buttons; 440 px (§9.9). |
| **Sheet** | Settings, shortcuts, print: things the user opened to change | `Dialog` with `size="lg"`, 640 px; sections of rows, body scrolls (§9.9). |
| **Toast** | Confirmation of an action not visible on screen, with Undo | Bottom-centre, 5 s, max one at a time. Most actions need no toast. |
| **Banner** | A problem that blocks work (library missing, model failed) | Top of the canvas, plain sentence + one action. |
| **StatusLine item** | Saved, background work, model state | Icon + short text; click for details. |
| **EmptyState** | A panel or list with nothing yet | One sentence of what it is for, one action, optional small illustration in line style. |
| **Skeleton** | Panel content still loading | Neutral blocks shaped like the content; never on the page itself. |
| **Kbd** | Shortcuts in tooltips and menus | Small, muted, monospace. |

---

## 9. Interaction patterns

### 9.1 Selection → action
Selecting text shows the **selection bar** above it (below when near the top): highlight colours first, then underline/strike/squiggly, comment, copy, cite, and (from M5) Explain. Icon buttons with tooltips; at most 10 items; it disappears when the selection is cleared or the user scrolls it away. Right-click on a selection opens a context menu with the same actions plus less frequent ones (§17.13).

### 9.2 Tools are modes with a way out
A tool stays chosen until the user picks another or presses **Esc** (back to Select). The pointer shows the tool (crosshair to draw, text cursor to mark). One-shot tools (sticky note, text on page) return to Select after use.

**Every tool is understandable on first use** (review R5):
- **Tooltip:** name, shortcut and what to do ("Highlight (H) — drag over text").
- **While a tool is chosen:** the status line says the same and how to stop ("Highlight: drag over text · Esc to stop").
- **Text tools** work as soon as the page shows: the page's text must be there (§17).
- **Mark colour:** the current colour is shown on the tool.

### 9.3 Saving and undo
- Every change is stored immediately; the status line says *Saved* for 2 s, then shows nothing. Never a spinner for local saves.
- Undo (`Ctrl+Z`) and redo (`Ctrl+Y`) work for every change to user content, survive restarts, and are offered instead of confirmations.
- Writing into the PDF is a separate, explicit act ("Save into PDF", `Ctrl+S`); a dot on its icon shows changes not yet in the PDF.

### 9.4 Errors
Plain sentence: what happened, and what to do. No codes in the main text (details on "More"). Errors appear where the action was taken (inline under a field, in the card) or as a banner if the whole view is affected. Never a modal for a recoverable error.

### 9.5 Waiting
| Time | Show |
|---|---|
| < 100 ms | the control's pressed state (always) |
| 100 ms – 1 s | the control shows *busy* (subtle) **and the first results as they arrive** |
| 1 – 10 s | results streaming in place, with a count that grows and what is being done ("searching page 240 / 759") |
| > 10 s | a background task in the status line, with progress and Cancel; the user keeps reading |

Partial results are never held back until the work is done (§17).

### 9.6 Keyboard
Every action has a place in the command palette (`Ctrl+K`); frequent ones have shortcuts, all remappable. Single-letter tool keys work only when focus is not in a text field. `Esc` always closes the innermost thing (card → popover → tool → selection).

### 9.7 Empty and first-use states
Each panel's empty state says in one sentence what it is for and offers one action (e.g. Notes: "Your notes on this paper. Select text and choose *Add to notes*, or start typing."). First-use hints appear once, inline, are dismissible, and never block the page. No tours.

### 9.8 Destructive actions
Prefer undo. Confirm only what cannot be undone (deleting a collection's contents from disk, clearing history), naming exactly what goes and what stays.

### 9.9 Dialogs and sheets (founder report, 2026-11-24: Settings spilled out of its dialog)
- **Dialog (440 px):** asks one thing; title, one paragraph, at most two buttons. **Sheet (640 px, `size="lg"`):** settings, lists, forms; sections of rows, one *Close*. Both are the kit's `Dialog`; nothing else opens over the app in the middle.
- **Whole on the screen:** never taller or wider than the window less 32 px. Title and buttons stay in place; what is between them scrolls.
- **Rows:** controls in neighbouring rows are at least 8 px apart, a button after a list too. In a narrow sheet a row's control wraps under its text.

### 9.10 Selecting text (founder report, 2026-11-24)
- **What is blue is what is copied.** The selection is drawn from the engine's character boxes, one even band per line with the spaces included, never from the browser's own selection of invisible text; copying, marking and Explain use the same characters.
- **The pointer finds the printed letter.** Invisible words are fitted to the printed word's width and height, at every zoom, whatever the font.

- What is selected stays selected while scrolling, zooming or resizing: the pages in view are moved and scaled, never made anew (they are kept by their number; a layout depends on sizes, not on the scroll position).

### 9.11 Pages of different sizes
- **Every page keeps its own size and direction.** At one zoom all pages are drawn to the same scale, so a landscape table is wide and an A3 plan is large, as in the paper; they are centred in the column. Fit width fits the widest page and fit page the largest.
- A page is never stretched: its box, drawing, thumbnail, text and marks always have the same shape.

---

## 10. Annotations, notes and sticky notes

Three ways to write about a paper, each with one clear job:

| | Annotation | Sticky note | Paper notes |
|---|---|---|---|
| **Job** | Mark something in the text | A thought at a spot on the page | Your own writing about the paper |
| **Lives** | On the text | On the page margin, at a point | Right panel, *Notes* tab (one notebook per paper) |
| **Made by** | Selection bar or a tool | Sticky-note tool (N) or "Comment" | Typing in Notes, or "Add to notes" from a selection or mark |
| **In the PDF** | Yes (standard annotation) | Yes (`/Text` note with popup) | Exported (Markdown now; in the PDF as an attached page later) |

### 10.1 Sticky notes: present, not loud
- On the page a sticky note is a **comment pin**: a small round bubble (16 px at 100 % zoom) in the note's colour whose one sharp corner points at the spot, with the author's initial inside, or the number of comments when the thread has replies. Marks that carry a comment show a smaller pin at their end. Text is not shown on the page by default.
- **Hover** shows the first two lines in a tooltip-like preview; **click** opens its card (text, replies, resolve).
- **Margin view** (toggle in the Notes panel or `Ctrl+Shift+M`): sticky notes and marks that carry a comment are shown as small cards in a margin column beside the page, aligned to their spot and stacked without overlap, with a thin leader line. Off by default; remembered per paper.
- **Show/hide all** (`Ctrl+H`): hides every annotation and note on the page for clean reading; the status line shows "Annotations hidden" so it is never a mystery.
- Resolved notes fade to 40 % and collapse to the icon; they can be filtered out.
- Sticky-note colours use the annotation palette; the default is Yellow.

### 10.2 Paper notes: a quiet notebook
- One notebook per paper, in the right panel's **Notes** tab; saved as you type; undoable.
- Writing surface: Markdown with light formatting (headings, bold, italic, lists, quotes, links, inline math `$…$`), shown formatted while typing: marks such as `#`, `**` and `>` are hidden on every line except the one being edited. No toolbar; formatting by typing Markdown or with `Ctrl+B/I`.
- Every save is kept as a version (one per minute of typing, 200 per paper); *Earlier versions* brings any of them back.
- **Quotes keep their place:** "Add to notes" inserts the selected text as a quote with a location chip (`p. 4`); clicking the chip goes to that spot and flashes the passage once. Marks can be added the same way (the chip opens the mark).
- **Backlinks:** a mark that is quoted in the notes shows a small note icon in its card; the Notes list can show "quoted in notes".
- Notes are searchable with the paper's annotations (one search box in the Notes panel) and exported in M7 (Markdown, Typst, BibTeX with notes).
- The notebook belongs to the paper, not to a file version: it survives "Save into PDF" and re-imports.
- Lab (Phase 2) uses the same component for project notes, with links to papers, figures and runs.

### 10.3 Annotation cards
One card for every kind (§8 Card): colour row, category, comment, replies, resolve, delete; free text adds text size; drawings add line width. The card opens next to the mark, never covers it, and closes on Esc or outside click.

### 10.4 Lists and filters
The Annotations list (left panel) and Notes (right panel) share filters: colour, category, author, page, resolved; search across mark text, comments and notes.

---

## 11. AI in the interface

AI helps when asked and then gets out of the way. These rules apply to every AI feature in every app.

1. **On request only.** No AI result appears unless the user asked (a click, a shortcut, a question). Background preparation may run, but its results stay hidden until requested. No "Did you know…?" suggestions over the page.
2. **Two places only:**
   - **Inline card** for things about a spot (Explain a term, a figure, an equation): opens next to the selection like an annotation card, with *Pin to notes*, *Copy*, *Ask more* and Close.
   - **Assistant tab** in the right panel for conversations (ask the paper, ask several papers). One assistant per window.
   Nothing else in the frame is AI-specific; no floating chat bubbles, no AI side bars that open by themselves.
3. **Marked, once.** AI actions use the `sparkles` icon and the word *AI* in their tooltip; AI text has a thin left rule in `--fg-muted` and a "Generated · model name" line. Nothing else is decorated (no gradients, no glow, no rainbow text).
4. **Evidence first.** Every claim that comes from the paper carries a location chip (`p. 7, Eq. 3`); clicking it goes there and highlights the passage. Answers that the checker could not verify say so in words ("Not found in the paper"), not just with colour.
5. **Calm streaming.** Text streams in at reading pace without moving the rest of the layout; no blinking cursor; the card or panel has a Stop button while generating.
6. **Short by default.** First answer ≤ 4 sentences or 5 bullets, with *More* to expand. The user's preferred length is remembered.
7. **State in the status line.** Model loading, downloading or unavailable is shown in the status line (`sparkles` icon + short text), with details on click — never as a modal or a banner over the page unless the user's action failed.
   - **Explain:** double-click a word, or `sparkles` in the selection bar. A double-click never opens a card that can only say "no model": without a model only the selection bar answers, with a way to Settings. Closing the card or Stop ends the answer.
8. **Respect the reading space.** Opening the assistant never changes the zoom or scroll of the page; inline cards never cover the text they explain; one AI card open at a time.
9. **The user stays in charge.** Every AI output can be copied, pinned to notes, or dismissed; nothing is written into the user's notes or the PDF without an explicit action.
10. **Privacy visible.** Anything that leaves the device (only with consent) shows a `cloud` icon and the destination before it is sent.

**Clutter budget.** Even with every AI feature, the default Reader view shows: top bar, tool rail, the page, and the status line. Panels and cards appear only on request. A feature that cannot fit this budget is redesigned, not squeezed in.

---

## 12. Writing for the interface

- **English only until Phase 3** (founder decision); every string goes through `t()` in `ui/kit/src/i18n.ts`.
- **Plain words**, short sentences, sentence case, no exclamation marks, no "please" in buttons, no jargon (*Save into PDF*, not *Flush annotations*).
- **Buttons are verbs** (*Save*, *Add PDF*, *Reply*); menu items are verbs or nouns, consistent in a menu.
- **Say what happens:** *Delete the collection “Fluids”? The papers stay in your library.*
- **Numbers and units:** `12 pages`, `p. 4`, `3 min`; thin space before units in technical values.
- **The product name** comes from `product.toml`; never hard-code it in strings.
- Tooltips: name + shortcut (`Highlight (H)`), no full sentences unless needed.

---

## 13. Accessibility

Required for every screen, checked in review and in M8's accessibility pass:
- Everything works with the keyboard; focus is always visible (2 px accent ring); focus order follows the visual order; `Esc` closes the innermost layer and returns focus to where it came from.
- Every control has an accessible name; icon-only controls use their tooltip name; live regions announce *Saved*, search counts and AI completion.
- Contrast targets of §3.1 in all five themes; nothing depends on colour alone; hover-only information is also reachable by focus.
- Text scales with the system and the app's text size setting (90–150 %) without breaking any part of the interface (§5.4, `e2e/layout.mjs`).
- Hit targets ≥ 28 px; drag actions (move, resize) have keyboard equivalents (arrow keys move the selected mark by 1 pt, Shift by 10 pt).
- `prefers-reduced-motion` and Windows high-contrast mode are honoured.
- (Phase 3) Right-to-left layouts mirror the frame; icons with direction (arrows, undo) mirror, others do not.

---

## 14. One family of apps

| App | Frame | What differs |
|---|---|---|
| **Reader** | Full frame (§2) | — (the reference implementation) |
| **Lab desktop** (Phase 2) | Same frame; canvas holds a paper, a method graph, a notebook or a run | Left panel adds Project; tool rail adds Lab modes; right panel adds Run and Validation tabs. Same tokens, icons, cards and status line. |
| **Web / server consoles** (Phase 3+) | Same tokens and components (ui/kit compiled for the web); no tool rail where there is no canvas | Responsive down to 360 px with panels as sheets. |
| **CLI and MCP** | Text only | Same words as the UI (*paper*, *annotation*, *library*); errors in the same plain style; colours only for status (green ok, yellow warning, red error) and off when not a terminal. |
| **Installer, first run, emails** | Light theme, same type and icons | — |

Rules: one `ui/kit` package for all apps; an app may add components to the kit, never fork them; screenshots of each app's main screens are compared in review to keep the family alike.

---

## 17. Responsiveness (binding, added 2026-11-15)

From the review of a 759-page book (`docs/design/review-2026-11-15-large-documents.md`).

### 17.1 Answer at once, refine later
- Every action shows a visible response within **100 ms**: pressed state, spinner, or a first partial result.
- **Streaming:** results appear as they are found (search hits, contents entries, LaTeX), and counts grow while the work goes on. A result may be corrected later (a sharper picture, a final count); it is never withheld until everything is finished.
- Long work says what it is doing and how far it is ("128 matches · searching 240 / 759").

### 17.2 What the user looks at goes first
The page in view, its text and links, a search being typed and a click come before background work: previews ahead, preparation, indexes, models. Background work runs in small pieces between them.

### 17.3 Budgets on a large book (750+ pages, while preparation runs)
| What | Budget |
|---|---|
| First picture of the page | < 1 s |
| Page text selectable (highlight, copy) | < 1.5 s |
| First search hit | < 1 s; hits keep arriving page by page |
| Sharp page after a jump | < 0.8 s |
| Blank pages while scrolling | none (a preview at least) |

### 17.4 Find
`Ctrl+F` opens a **find bar** over the page:
- it shows "*n / m*", Previous and Next (Enter / Shift + Enter), and a close button;
- the count grows while the search runs;
- the side panel keeps the full list with options.

### 17.5 Where you are
- **While scrolling,** a small page indicator ("46 / 759") shows **next to the scroll bar's thumb**, moving with it, and fades out 1 s after scrolling stops.
- **Dragging the scroll bar thumb** shows the page it will land on beside the thumb, in a slightly larger label, as other readers do.
- **Zoom:** Ctrl + wheel, a touchpad pinch and a touch-screen pinch all zoom around the pointer or the fingers. The view is scaled at once, then the pages are redrawn sharp.

### 17.6 Splits and panels
- **Resizing:** every split (side panels, two panes) has a drag handle; a double-click restores the default.
- **Sizes and minimums:** sizes are remembered; each pane keeps a minimum width at which it still works.

### 17.8 Touch screens, touchpads and pens (binding, added 2026-11-15)
Every app works fully on touch laptops and with precision touchpads, not only with a mouse.
- **Gestures:**
  - two-finger pinch on a touchpad or a touch screen zooms the page around the fingers, never the whole interface;
  - one finger (touch) or two fingers (touchpad) scroll;
  - double-tap zooms in to fit width, or back.
- **Taps:**
  - a tap is a click;
  - a long press opens what a right-click opens;
  - anything that opens on hover also opens on a tap (popovers, previews, tooltips with information).
- **Sizes:**
  - on a coarse pointer (`@media (pointer: coarse)`) controls, drag handles and the tool rail grow to at least 40 px targets;
  - selection and resize handles get a larger hit area.
- **Pens and touch:** drawing tools, highlighting, selection and the region tool use pointer events, so mouse, pen and touch behave the same; palm and second fingers do not draw.
- **Tests:** end-to-end checks use real gestures (DevTools `Input.synthesizePinchGesture` for touchpad and touch, `Input.dispatchTouchEvent` for taps and drags) as well as the mouse.

### 17.9 Printing (revised 2026-11-18)
**Printing prints the paper, never the window, and works on every machine.** Ctrl+P and Print open the Reader's **print sheet**, then the **standard Windows print dialog** (printer, copies, paper, both sides):
- **Pages:** all, the current page, or a range ("1-5, 8, 11-13").
- **Annotations:** included or not (on by default when the paper has some).
- **Drawing:** the chosen pages are drawn sharp (about 200 dpi), at the page's own size and orientation, with progress and Cancel in the sheet. The print dialog then shows exactly those pages.
- **Ctrl+P is the Reader's:** the browser's own print of the interface is never reachable.

**Why not the system's "print this file" command** (tried 2026-11-15, failed for the founder): that command asks the default PDF app to print. After installing, the default PDF app is the Reader itself, which registers no print command, so Windows reports an error. On a machine without a PDF app it fails too. The Reader therefore prints its own drawings through the standard dialog and depends on no other app.

**Checked by:** an end-to-end test prints to PDF through the same path (DevTools `Page.printToPDF`). The result must have exactly the chosen pages, in order, each at its page's size, with the annotations when asked.

**Later (documented, not built — waiting for the founder's go):**
- **Print a selection or an object:** a figure, table or equation (from its popover), or the selected region only.
- **Notes and comments:** print the paper with a comments list at the end (each comment with its page and quote), or the paper notebook on its own.
- **Layout:** pages per sheet (2, 4), booklet, fit to paper or actual size, margins for binding.
- **Reading aids:** grayscale or "ink saver" (paper colour kept white, dark themes not printed), a header or footer with the citation and page number, line numbers.
- **Export instead of printing:** save the chosen pages, with or without annotations, as a new PDF (shares the drawing path).
- **Remembered choices:** the last pages, annotations and layout per paper.

### 17.10 The library (added 2026-11-15)
- **Two views, remembered:**
  - **List:** text, dense — title, authors, year, pages, added;
  - **Covers:** a grid of first-page thumbnails with title and authors, like a shelf. Laid out on the 4 px grid (§5.1): 16 px from the edges (aligned with the tool row above), 24 px between covers, the cover at the page's own proportions with its page shadow.
- **Rename** (F2 or the row menu): the title shown in the library and tabs. The file name stays as the original name.
- **Remove from library** (Delete or the row menu) takes a paper out at once, with **Undo** for 10 s in a toast; no confirmation dialog (§9.8: undo instead of confirm). Its notes and annotations go with it and come back with Undo. The managed copy is deleted when the library is next opened, unless Undo was used.

**Folders and several papers at once (founder request, 2026-11-24: "the page of my PDFs is crowded")**
- **Home** shows the folders first, then only the papers in no folder; *All papers* shows everything. A folder shows its folders, then its papers, with the way back above. Folders can be made anywhere, inside folders too, renamed, moved and deleted (papers stay).
- **Selecting:** a check mark on each paper (on hover, always on touch), Ctrl+click adds, Shift+click a range, Ctrl+A all, Esc none. While something is selected a click selects. The bar in place of the tools says how many and what can be done: *Move to*, *Suggest folders*, *Remove*, *Clear*.
- **Moving:** by menu, or by dragging onto a folder with mouse or pen (a touch scrolls). Everything that moves or removes has Undo.
- **Suggest folders** (`sparkles`, AI): shows the groups with their papers before anything happens. Names can be changed, papers put elsewhere, groups left out; only *Create folders and move* does it. Papers it is unsure about stay where they are. Stop ends the model.

### 17.11 Asking the library and summaries (founder request, 2026-11-24)
- **Ask the library** is the first entry of the library's list (`sparkles`; Ctrl+Shift+L from anywhere). One box for a question or a description, in any language; Enter asks, Shift+Enter is a new line, Esc or *Stop* ends it.
- **At once, then better:** what the question's own words find is shown within a moment, before any model answers. A line says what is being worked out. Papers, places and the nearest ones replace it as they come; a newer question ends the older one.
- **Order:** *Nearest* first (cards, with the picture of a figure, table or equation as it looks on the page), then the papers with why they were chosen and their places, then where the words also are. A place is shown once.
- **Words of the paper:** every quote stands in its paper word for word, with the searched words set off. Only *why* is written by the model; the last line says which model chose, or that words alone were used.
- A place opens its paper at its page and flashes; the results are still there on the way back.
- **Summaries** belong to the papers and are kept with them. The quick one (headings, captions, main words) is made by itself in the background; the written one on request: for one paper (*Summary* in its menu), for the selected ones, or for all. It can be read while it is made, stopped and gone on with, and it never makes the person wait for an answer.
- **The sheet** shows what the document is, its subjects, questions it can answer (a click asks one), then every section with its pages, role, summary and tags. A summary that is the paper's own words says *Quoted from the paper*.
- *Suggest folders* (§17.10) uses the summaries where there are some.

### 17.12 Understood by AI (founder request, 2026-11-24)
- **Brain mark** (`brain`): a paper the app has understood shows it, in the list, on its cover, in the accent colour. While a paper is being read its mark pulses (not with reduced motion) and says the section in its tooltip. A paper whose turn has not come shows nothing. A click opens the summary.
- **By itself and out of the way:** papers are understood in the background, the one opened last first. Nothing opens, nothing is asked, no message for what simply works; the status line says *Understanding papers: n of m* with a menu (pause, go on, settings). When all is done it disappears.
- **Never in the way:** reading, scrolling, explaining and asking come first; on a low battery it waits and says so (*Go on anyway* in its menu).
- **Closed too early?** It goes on the next time, where it stopped.
- **One model:** Settings show the order of the models (first choice, if that does not work, …) with what each needs. When another than the first choice answers, the app says so once, with the reason, and offers the order.
- **A failed answer** is said in words a person understands, with *Try again*; never the text of an error of a program.

### 17.13 Explaining things on the page (added 2026-11-25)

1. **Ways in, the same everywhere:** right-click on the page, *Explain* (`sparkles`) in the popover of a figure, table, equation or region, the selection bar, the command palette. A right-click never selects, scrolls or opens AI by itself; it opens a menu.
2. **The menu names what is under the pointer** ("Explain Figure 3", "Explain this paragraph"), AI first, then what can be copied, then notes. With a selection under the pointer it is about the selection. At most 9 rows; a row that cannot work now is not shown.
3. **One card** (§11.2). What code knows is there at once: the symbols of an equation, what an explanation is based on. The text streams below. Nothing that is there moves when more comes.
4. **Honest about the picture:** "From the caption and the text. The picture itself was not looked at." with *Look at the picture*. Looking asks once per paper, in a dialog that says what is read, by which model, that nothing leaves the computer and that answers pause meanwhile; *This figure* or *All n figures*. What was seen is marked *Seen in the picture*, apart from what the paper says.
5. **Symbols** are a small table: symbol as printed, meaning in the paper's words, unit, a chip to the place. No meaning found: "not explained in the paper", in muted text.
6. **Units check** in words: "Units agree", "Units do not agree: left m/s, right m", "Units not known for: β". Never only a colour.
7. **Asking a table** happens in the table view: a question box under the header; the answer is a sentence with the number, the cells used are outlined and the answer's cell is filled; clicking the sentence scrolls to the cell. The sentence says what was computed.
8. **Simpler** and **Deeper** are quiet buttons under a finished answer; they answer again in the same card.

### 17.7 Transient things
Hover popovers and previews close by themselves when:
- the pointer leaves both the trigger and the popover (after 300 ms);
- the view scrolls or zooms;
- Esc is pressed, or the user clicks outside.

---

## 15. Governance and review

- **Source of truth:** this document for rules, `ui/kit` for implementation (tokens, components, icons, strings). If they disagree, fix one of them in the same change.
- **Changing a rule:** update this document (version and date at the top) in the same pull request as the code; a change of principle (§1) or of the icon set needs an ADR.
- **Every UI change** is checked against §16 before merging and adds or updates an end-to-end check for the behaviour.
- **Automated checks** (from milestone MD): all colours from tokens (a lint rule over CSS), every icon-only button has a name (a DOM check in the e2e scripts), contrast of every token pair in every theme (a unit test), no string outside the string table (existing check).
- **Design review at each milestone end:** screenshots of the changed screens in Light and Dark, a run of the checklist, and a note in the milestone status.

---

## 16. Review checklist

Copy into the review of any UI change.

- [ ] The page is still the calmest, highest-contrast thing on screen.
- [ ] Nothing new appears without a user action; nothing moves while reading.
- [ ] Reuses kit components and tokens; no raw colours, sizes or fonts; no width on content (§5.4).
- [ ] The recipes of `docs/best-practices.md` for what was made (page, pop-up, dialog, panel, setting) are followed.
- [ ] Fits at 800 × 500 and at 150 % text with the longest text; the new part is in `e2e/layout.mjs`.
- [ ] Icons from the set, right size, tooltip with name and shortcut, accessible name.
- [ ] Top bar still fits on one row at 1280 px; default view within the clutter budget (§11).
- [ ] Works in Light, Sepia, Dark, Night and High contrast.
- [ ] Keyboard: reachable, visible focus, Esc closes, shortcut in the palette.
- [ ] Every state covered: empty, loading, error, offline, busy, disabled.
- [ ] Text: plain, sentence case, verbs on buttons, through `t()`.
- [ ] Changes saved at once and undoable; confirmation only for the irreversible.
- [ ] AI (if any): on request, marked with `sparkles`, evidence chips, Stop, short by default.
- [ ] Answers within 100 ms; results stream; budgets of §17.3 met on the large book while preparation runs.
- [ ] Transient UI closes on scroll, pointer leave and Esc; splits resizable.
- [ ] Print prints the paper through the print sheet and the standard dialog, checked by printing to PDF (§17.9); library actions undoable (§17.10).
- [ ] Works with touch and touchpad (§17.8): pinch zooms the page, taps replace hover, targets large enough on coarse pointers.
- [ ] End-to-end check added or updated, with **real input** (mouse, wheel, keys, pinch, touch) where the user would use them.
