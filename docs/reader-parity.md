# Reader parity checklist

Task P0.C.4. Source: Draft 2.2 §28.3 (parity with the ElectroChat 2.5 reader, improved) and §28.4 (new T1 capabilities).
Each line has an end-to-end test id (`e2e/reader/<id>.spec.ts`) and the Phase 1 milestone that delivers it.
Reader 1.0 ships only when every line is checked (§28.8). Lines marked *(1.x)* may follow in an update.

Legend: **P** = parity with today's reader · **+** = improvement or new in T1

## §28.3 Parity, improved

### Opening — M2
- [ ] P `open.dialog` Open a PDF through the dialog
- [ ] P `open.association` Open with the app from Explorer (file association), second instance hands off to the running app
- [ ] P `open.managed-copy` Added PDFs are managed copies; original byte-identical afterwards
- [ ] + `open.drag-drop` Drag and drop a PDF onto the window
- [ ] + `open.recent` Recent files list
- [ ] + `open.from-link` Open from a link in chat or a report
- [ ] + `open.tabs` Several PDFs in tabs

### Pages — M2
- [ ] P `pages.nav` Previous/next, page number box, page count
- [ ] P `pages.keys` Arrows, Home, End, Space, PgUp/PgDn
- [ ] + `pages.continuous` Continuous scroll with virtualised pages, 60 fps on 100 pages
- [ ] + `pages.thumbnails` Thumbnails sidebar
- [ ] + `pages.modes` Single and two-page modes
- [ ] + `pages.resume` Reading position remembered per paper

### Zoom — M2
- [ ] P `zoom.range` Zoom range, fit width, fit page
- [ ] P `zoom.keys` Ctrl +, Ctrl −, Ctrl 0
- [ ] + `zoom.gesture` Pinch and Ctrl+wheel with the anchor under the cursor
- [ ] + `zoom.smooth` No blank frames during zoom; sharp re-render after
- [ ] + `zoom.selection` Zoom to selection

### Contents — M2 / M4
- [ ] P `toc.pdf` Table of contents from PDF bookmarks (M2)
- [ ] + `toc.generated` Generated outline from detected sections when the PDF has none (M4)
- [ ] + `toc.objects` Jump list of figures, tables, equations (M4)

### Find — M2
- [ ] P `find.basic` Find text (Ctrl+F)
- [ ] + `find.results` Results list with context and match count
- [ ] + `find.options` Case and whole-word options
- [ ] + `find.all-tabs` Search across all open PDFs
- [ ] + `find.term-usage` "Where else is this term used" (M6)

### Links — M2 / M6
- [ ] P `links.internal` Ctrl+click an internal link goes to the target
- [ ] P `links.external` External links open in the browser after showing the target
- [ ] + `links.history` Back/forward after following links
- [ ] + `links.hover-preview` Hover preview of the link target region
- [ ] + `links.citation-card` Citation hover card: title, authors, year, abstract, open-access link (M6)

### Select tool — M2 / M4
- [ ] P `select.ask` Select text to ask about it (M5)
- [ ] + `select.copy` Copy as plain text in reading order
- [ ] + `select.copy-cite` Copy with citation (M3)
- [ ] + `select.copy-latex` Copy equation as LaTeX (M4)

### Region tool — M4
- [ ] P `region.drag` Ctrl+drag to select an area to ask
- [ ] P `region.detect` Exact object detection: equation, figure, table, paragraph, graph
- [ ] + `region.fast` Click to object outline < 50 ms
- [ ] + `region.snap` Snap to object and adjustable crop

### Highlight — M3
- [ ] P `hl.basic` Highlight
- [ ] + `hl.colours` Colours
- [ ] + `hl.categories` Categories: definition, method, result, question, to check
- [ ] + `hl.underline-strike` Underline and strike-out

### Comment — M3
- [ ] P `comment.add` Add a comment
- [ ] + `comment.thread` Threaded replies
- [ ] + `comment.resolve` Resolved state

### Text on page — M3
- [ ] P `text.add` Write text on the page
- [ ] + `text.style` Font size and colour

### Draw — M3
- [ ] P `draw.ink` Ink drawing
- [ ] + `draw.shapes` Shapes and arrows
- [ ] + `draw.eraser` Eraser

### Undo and save — M3
- [ ] P `save.undo` Undo the last change
- [ ] P `save.pdf` Save marks into the PDF (Ctrl+S) < 200 ms
- [ ] P `save.indicator` Unsaved indicator
- [ ] + `save.history` Full undo/redo history, surviving restart
- [ ] + `save.autosave` Autosave of marks to the library
- [ ] + `save.interop` Standard annotations: open correctly in Acrobat; Acrobat's marks open in the Reader

### Write with AI — M5
- [ ] P `write.ai` Write with AI on the page, with continuation
- [ ] + `write.policy` Policy and evidence rules applied; result placed as an annotation

### Explain (double-click) — M5
- [ ] P `explain.popup` Compact popup on double-click; closes on outside click
- [ ] P `explain.checked` First answer, then checked against the PDF and corrected if needed
- [ ] P `explain.pages` Page buttons in the popup
- [ ] P `explain.to-chat` "Ask more in the PDF chat"
- [ ] + `explain.fast` First token < 500 ms (warm); checked < 2 s
- [ ] + `explain.language` Explain in Persian or English — **moved to Phase 3** (English-only until then)
- [ ] + `explain.depth` "Explain simpler / deeper"
- [ ] + `explain.save-note` Save the explanation as a note

### Explain figure — M5
- [ ] P `fig.explain` Explain a figure from the saved scan, no model switch, < 1.5 s
- [ ] + `fig.jump` Figure jump list (M4)
- [ ] + `fig.compare` "Compare with figure N"

### Right-click menu — M5
- [ ] P `menu.explain-object` "Explain this paragraph / equation / figure / table / graph" from the detected object
- [ ] P `menu.ask-part` "Ask about this part"
- [ ] P `menu.lookup` "Look up selected text"
- [ ] P `menu.comment` "Add comment"
- [ ] P `menu.region` "Select area to ask"
- [ ] + `menu.latex` "Copy as LaTeX"
- [ ] + `menu.csv` "Table to CSV"
- [ ] + `menu.term-usage` "Where else is this term used" (M6)
- [ ] + `menu.cite` "Cite"
- [ ] + `menu.experiment` "Make an experiment from this method" — shown only with the Lab installed (Phase 2)

### Equations — M4 / M5
- [ ] P `eq.latex` Equation to LaTeX (math OCR) (M4)
- [ ] P `eq.numbers` Equation numbers; find an equation by its number (M4)
- [ ] P `eq.context` Context and symbol definitions (M5)
- [ ] P `eq.image` Equation image (M4)
- [ ] + `eq.symbols` Symbol table per equation (M5)
- [ ] + `eq.dimensions` Dimensional check where units are known (M5)
- [ ] + `eq.notebook` "Open in notebook (SymPy)" — exports a file (M5)
- [ ] + `eq.plot` "Plot this equation" — exports a file (M5)

### Tables — M4 / M5
- [ ] P `table.answer` Answers computed from table rows and columns (M5)
- [ ] + `table.export` Export to CSV (M4)
- [ ] + `table.view` Sortable table view (M4)

### Figures — M4 / M5 / M7
- [ ] P `figs.index` Figure index with captions (M4)
- [ ] P `figs.vision` "Inspect with vision": batch, with consent (M5)
- [ ] + `figs.data` *(1.x)* Data points from simple line and scatter plots (M7)

### Scanned pages — M4
- [ ] P `ocr.local` Local OCR
- [ ] + `ocr.layer` Text layer added (derived copy) so selection and find work

### PDF chat — M6
- [ ] P `chat.multi` Several chats per PDF, new chat, history, stop
- [ ] P `chat.cites` Answers cite pages
- [ ] P `chat.open-mark` Clicking a cited page opens and marks the passage
- [ ] P `chat.web-fallback` Trusted outside sources when the PDF cannot answer, with network permission
- [ ] + `chat.collection` Chat over several PDFs or a collection
- [ ] + `chat.cards` Answer cards linking to every cited passage
- [ ] + `chat.outside-label` Clear label when an answer comes from outside the PDF

### Background preparation — M4
- [ ] P `prep.background` OCR, equations, section index and figures prepared in the background
- [ ] P `prep.progress` Progress shown
- [ ] P `prep.resume` Resumable after restart
- [ ] + `prep.cpu` On the CPU; the model stays fast
- [ ] + `prep.stages` Per-stage progress
- [ ] + `prep.never-blocks` Reading never blocked; 20-page paper prepared < 10 s

### Panels — M2+
- [ ] P `panels.toggle` Show or hide the questions panel
- [ ] + `panels.resizable` Resizable side panels: chat, notes, outline, thumbnails, paper card (method graph slot hidden until Phase 2)

### Workspace — M2 / M7
- [ ] P `ws.split` Split view and tabs
- [ ] P `ws.add-collection` Add the PDF to a project or collection (M7)
- [ ] + `ws.add-project-tags` "Add to project" with tags (M7)
- [ ] + `ws.drag-to-lab` Drag a PDF tab into the Lab (Phase 2)

### Other — M2 / M7
- [ ] P `other.usual-app` Open in your usual PDF app
- [ ] + `other.print` Print through the system dialog
- [ ] + `other.export-pdf` Export the annotated PDF (M7)

## §28.4 New in T1

### Reading — M2
- [ ] `read.themes` Dark, sepia and high-contrast page rendering
- [ ] `read.rtl` RTL-friendly UI; English and Persian — **moved to Phase 3** (English-only until then)
- [ ] `read.stats` Reading statistics: time spent, pages read, last position
- [ ] `read.bookmarks` Named bookmarks

### Understanding — M5 / M7
- [ ] `und.paper-card` Paper card with every line linked to its page (M7)
- [ ] `und.glossary` Glossary of abbreviations and terms, linked to definitions (M7)
- [ ] `und.method-panel` Method panel — **Phase 2** (slot exists, hidden in 1.0)
- [ ] `und.translate` Translate selection English ↔ Persian, equations intact (M5) — **moved to Phase 3** (English-only until then)
- [ ] `und.multi-paper` Ask about several papers (M7)

### References — M6
- [ ] `ref.parse-resolve` Bibliography parsed; each entry resolved (DOI, arXiv) with title, abstract, open-access link
- [ ] `ref.open` One click opens the cited paper in a new tab (open access and allowed only)
- [ ] `ref.citing-related` "Papers citing this one" and "related papers"

### Notes — M3 / M7
- [ ] `notes.sidebar` Annotation sidebar with filters (colour, category, page, author) and search (M3)
- [ ] `notes.export` Export highlights and notes to Markdown or Typst with citations and pages (M7)
- [ ] `notes.bibtex` Copy BibTeX for the paper; cite selection with page number (M7)

### Notes and interface — MD (founder request, 2026-11-14)
- [ ] `notes.paper` Paper notes: one notebook per paper in the right panel, saved as you type (MD)
- [ ] `notes.quote` "Add to notes" from a selection or mark, with a chip that jumps back to the place (MD)
- [ ] `notes.sticky` Sticky notes: small icon on the page, hover preview, card on click (MD)
- [ ] `notes.margin` Margin view of sticky notes, off by default (MD)
- [ ] `ui.hide-marks` Hide/show all annotations for clean reading (MD)
- [ ] `ui.focus` Focus mode: page only (MD)
- [ ] `ui.icons` Tools as icons with tooltips and shortcuts; one-row top bar; status line (MD)

### Bridges to the Lab — M7 / Phase 2
- [ ] `bridge.experiment` "Make an experiment from this method" — contract in M7, live in Phase 2
- [ ] `bridge.add` "Add to project", "Add to collection" (M7)
- [ ] `bridge.track` "Track this paper" — T2

## §28.8 Acceptance (non-feature lines)
- [ ] `acc.budgets` All §28.7 budgets met on the minimum hardware
- [ ] `acc.explain-quality` Explain golden set at least as good as the current app
- [ ] `acc.offline` Reading fully offline; AI features say what they need
- [ ] `acc.memory` < 150 MB per open 30-page PDF
