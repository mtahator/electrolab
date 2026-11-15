# Review 2026-11-15 — a 759-page book: what broke, why the documents and tests let it through, and the rules that prevent it

Version 1.0 · 2026-11-15 · Phase 1, M4 · Owner: Reader + Document engine · Feeds: design language §7, §9 and new §17; programming guideline Part 4 and Part 5.4; contract `document.search`, `document.render`; phase-1 plan tasks P1.M4.RSP.*

The founder opened a 759-page book (18 MB, two-level numbering "Figure 2.3", many tables) in the installed Reader and reported a list of problems. Each was reproduced on the same file before anything was changed. The measurements below are from this laptop (the minimum hardware), with the release build.

## 1. What was reproduced

| # | Report | Measured | Verdict |
|---|---|---|---|
| 1 | Search takes about 30 s | "a" right after opening: **27.9 s**, then only **50 of 3,654** hits shown | reproduced |
| 2 | Results should appear while searching, and a find bar with *n / m*, next and previous | Results appear only when everything is searched; find exists only as a side panel | missing behaviour |
| 3 | Highlight does not work, the tools are unclear | Page 1's text not in the page after **16 s**; with no text, the highlight tool has nothing to mark | reproduced |
| 4 | Pages load slowly compared with other readers | First picture 2.2 s; first sharp page waits behind background work | reproduced |
| 5 | Zoom with a trackpad pinch or Ctrl + wheel, around the mouse | Ctrl + wheel works with real input (136 → 234 %), each wheel step lays out all 759 pages again (sluggish); pinch depends on the touchpad driver | partly reproduced |
| 6 | Show the page number while scrolling | Only in the top bar | missing behaviour |
| 7 | Split views must always be resizable and responsive | The two panes are fixed at half each | missing behaviour |
| 8 | The figure preview sometimes stays when moving the mouse or scrolling | The hover popover had no "dismiss on scroll / leave" rule | reproduced |
| 9 | Figure numbers like 2.3 are wrong | Every "Figure 2.x" is labelled "Figure 2"; sentences "Table 1.1 provides …" are taken as captions; "Table 1.7 (Continued)" is listed again | reproduced |
| 10 | The blur is nice but should be a motion blur that follows the scroll speed, none when still | Previews are blurred the same at rest and in motion | missing behaviour |

## 2. Causes, and where each came from

| # | Cause in the code | Document or contract that allowed it | Why the tests did not catch it |
|---|---|---|---|
| 1, 3, 4 | **One PDF thread serves everything in arrival order.** Previews of all 759 pages, the whole paper's text for preparation, the search index (a second full text extraction) and the page geometry all queue in front of the page being read. | ADR-003 ("PDFium on one owning thread") says nothing about **priorities**. Programming guideline Part 4 lists "visible first" for drawing only. | Every timing test ran **one thing at a time** on papers of ≤ 286 pages. None ran search or text selection *while* preparation and previews were running. |
| 1 | **Search answers only when everything is searched**, and the index needs every page's text first. | `document.search` contract: one request, one answer. The search design note assumed a prepared index (`PaperNotPrepared`) and measured warm searches only. | The search benchmark measured a warm index (µs per query). No test measured "time to first hit" on a cold, large document. |
| 1 | The UI asked for 500 hits but the engine's default **50 hits per term** cut the list. | Contract default `max_hits_per_term: 50`, not overridden by the find panel. | The find e2e used words with fewer than 50 hits. |
| 3 | The page's text is requested in `requestIdleCallback` **without a time limit**; while the app is busy decoding pictures, "idle" never comes. | No rule for how long the text layer may take. | No e2e step selected text right after opening a large document. |
| 2, 6, 7, 10 | Behaviour not specified. | Design language: find only as a side panel (§2); page number only in the top bar; split view "side by side" without resizing; §7 "the page never animates" (written before the founder asked for motion). | Tests check what the documents ask for. |
| 5 | Every Ctrl + wheel event sets a new zoom immediately: the whole layout and the drawings are redone per event. | §7 forbade zoom animation, so no "zoom gesture" design existed. | The e2e zoomed by keyboard and buttons, never with wheel or pinch input, never checked the point under the mouse. |
| 8 | The hover popover closed only on a click outside or Esc. | §8 Components: popover rules list opening, not dismissal on scroll or pointer leave. | The e2e opened popovers by click and closed them with Esc. |
| 9 | Caption pattern accepted only whole numbers (`[0-9]+`); any line starting with "Table 1.1" counted as a caption, also inside a sentence; continuation captions were new objects. | Phase-1 plan OBJ.2 example labels ("Fig. 3", "Table 2") were all one-level. | All fixture papers are journal papers with one-level numbering; the non-academic set (M4) had no book-style numbering. |

## 3. Rules adopted (binding, in the documents named)

**R1 — Answer at once, refine later** (design language §17.1). Every action shows a visible response within 100 ms:
- a pressed state, a spinner, or a first partial result;
- results stream in as they are found, and counts grow while work goes on ("128 … searching 240 / 759");
- nothing waits silently;
- a result may be corrected seconds later (a sharper picture, a final count). It must not be withheld until everything is done.

**R2 — What the user is looking at goes first** (programming guideline Part 4.2). Every shared resource (the PDF engine thread, model sessions) has an **interactive** lane and a **background** lane:
- the interactive lane is the page in view, its text, links, a search the user typed, a click;
- background work (previews ahead, preparation, index, page geometry of the whole paper) runs only when the interactive lane is empty, in small pieces.

**R3 — Large documents are the normal case** (programming guideline Part 5.4, testing). Every feature meets its budget on a **750+ page book** while preparation runs:
- first page picture < 1 s;
- page text selectable < 1.5 s;
- first search hit < 1 s, with hits arriving as pages are searched;
- sharp page after a jump < 0.8 s;
- no blank page while scrolling.

**R4 — Transient things go away by themselves** (design language §8). Hover popovers and previews close when:
- the pointer leaves both the trigger and the popover (after 300 ms);
- the view scrolls or zooms;
- Esc is pressed, or the user clicks outside.

**R5 — Every tool is understandable on first use** (design language §9.2). The tool rail shows each tool's name and what to do in its tooltip:
- the status line says what the chosen tool does ("Highlight: drag over text · Esc to stop");
- the pointer changes;
- a tool never waits silently for something that is not ready: text tools work as soon as the page shows (R3).

**R6 — Every split can be resized** (design language §2). Panes and side panels have a drag handle (double-click resets). Sizes are remembered, and every pane stays usable down to its minimum width.

**R7 — Labels as printed** (contract, Document). Figure, table and equation labels keep their full printed number:
- "Figure 2.3", "Table A.1", "Figure S2";
- a caption is a label followed by caption text, not a sentence that starts with the label ("Table 1.1 provides …");
- "(Continued)" continues the earlier object rather than adding one.

**R8 — Motion follows the hand** (design language §7, amended):
- previews blur only along the scroll direction and only in proportion to the scroll speed, none at rest;
- jumps glide only over short distances;
- zoom gestures scale what is shown at once, then the pages are redrawn sharp.

## 4. New tests (so the same kind of problem is caught)

| Check | Where |
|---|---|
| A generated **800-page book** with two-level numbering, many figures and tables, long text (`fixtures/documents/generate.py` → `book-800.pdf`, licence-free) is part of the fixtures | `fixtures/documents` |
| **Under load:** open the book, and while preparation and previews run measure the R3 budgets (first picture, selectable text, first search hit, all hits streaming, sharp page after a jump) | `e2e/large.mjs` |
| Real input: Ctrl + wheel and pinch (`Input.dispatchMouseEvent` with Ctrl) zoom around the pointer; the point under the pointer stays under it | `e2e/large.mjs` |
| Highlight with the tool by dragging (real mouse input) right after opening | `e2e/large.mjs` |
| Popover closes on scroll and on pointer leave | `e2e/large.mjs` |
| Find bar: *n / m*, next, previous, Enter / Shift + Enter, results grow while searching | `e2e/large.mjs` |
| Labels: "Figure 2.3", "Table A.1", sentences that start with a label, "(Continued)" | core unit tests, `non_academic_documents` (book) |
| Streaming search: partial results arrive before the last page is searched | facade test |
| Engine priority: an interactive request is answered quickly while 700 background requests are queued | adapter test |

The founder's own book stays outside the repository (copyright). `e2e/large.mjs` runs on it as well when it is given with `EP_LARGE_PDF`.

## 4a. Founder feedback after the first fixes (same day)

- **Motion blur on every movement makes reading hard.** It now applies only while flying through several pages (§7).
- **Show the page beside the scroll bar while dragging it** (§17.5).
- **Pinch zoom on the touchpad and on touch screens; everything must work on touch laptops.** This is now a binding rule (§17.8) with gesture tests. Cause: the earlier design and tests assumed a mouse; the browser's own pinch zoomed the whole interface instead of the page.

## 5. Results after the fixes (same laptop, release build, while preparation runs)

`e2e/large.mjs` on the generated 800-page book and on the founder's 759-page book: **every check passes on both**.

| Budget (§17.3) | Before | 800-page book | Founder's book |
|---|---|---|---|
| First picture < 1 s | 6.1 s | 0.72 s | 0.96 s |
| Text selectable < 1.5 s | not after 16 s | 0.91 s | 0.10 s (after a jump to a text page) |
| First search hit < 1 s | 28 s (all at once) | 0.13–0.5 s, growing | 0.22 s; all 3,654 hits in 1.7 s |
| Sharp page after a far jump < 0.8 s | > 10 s | 0.6–0.7 s | 0.73 s |
| Highlight by dragging | did not work | works | works |
| Ctrl + wheel zoom at the pointer | sluggish | point stays (drift 0) | point stays |

Causes found while measuring, beyond the first table:
- **The link layer covered every page** and took the pointer, so text could not be selected or highlighted and mentions could not be hovered. Old tests clicked elements from script, which bypasses what lies on top; the new check `text_not_covered` uses the element under the pointer.
- **The app's own request queue** let previews fill both of its slots. It now has separate slots for background work.
- **Several commands each started the same whole-paper analysis at once.** It now runs once per file, in the background lane.
- **A saved place could be applied before the layout existed.** It is now applied when the layout is ready, and kept when a panel or split changes the scale.

**One search:** Ctrl+F, the search button and the Find list are one search (one query, one result, one current match). The bar is one compact row.
