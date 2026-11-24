# Project map

The short memory of the project: what exists, how it works, where it lives, what went wrong before. It is made to be **searched, not read**.

- `cargo xtask topics` lists every topic with its search words; `cargo xtask topics search` prints one and what it connects to.
- `cargo xtask topics --changed` prints the topics a change touches, what to read with them, and the checks to run.
- Without the tool: `grep -n -i "tags:.*zoom" docs/project-map.md`.

How to work with it:
1. Find the topic. Read it, the contract or note it names, and the topics under `connects:`, so the parts stay consistent.
2. Follow the recipes under `practice:` (`docs/best-practices.md`, `cargo xtask practices <recipe>`): they say what to do every time for a page, a pop-up, a dialog, a command, an engine, a bug, a release …
3. Write a new best practice under its own topic (`best:`), and in the recipe when it holds for every thing of that kind, with the test that proves it; write that test and see it fail once.
4. Run the checks of the changed topics. `cargo xtask release` runs everything and builds the installer only when all of it passes; CI runs what needs no desktop.

Rules: one topic = one `## #tag`, with `tags:`, `connects:`, `practice:`, `best:` (each names its test) and `check:`. The details stay in the contracts, ADRs and design notes. A change that makes a line wrong updates it in the same commit. `cargo test -p xtask` checks paths, connections and tests.

---

## #start Where to begin
tags: overview, layout, rules, plan, milestones, status
connects: #commands #git #bus
practice: command engine bug
- Governing plan: `../ElectroPaper-Lab-Plan.md` (Draft 2.2); phases and milestone status: `../plans/phase-1-reader-mvp.md`.
- Order of work: docs first (plan, contract, ADR, design note), then code, then tests.
- Layers: `foundation/` → `engines/<name>/{contract,core,adapters,facade}` → `hosts/` → `ui/`. Engines know each other only through contracts, over the bus (`cargo xtask arch-check`).
- UI rules are binding: `docs/design/design-language.md`. Interface language is English only until Phase 3.
- Done: M1, M2, M3, MD, M4 (open M4 items in the plan). Running: M5 (#inference, #explain).
- best: layers are checked by a tool, not by review — `test:clean_graph_passes` `test:breaking_the_engine_order_fails`
check: `cargo xtask arch-check`

## #commands Build, test, run
tags: build, cargo, test, lint, clippy, gen, e2e, installer
connects: #start #natives #git
practice: test release
- `cargo xtask fetch-natives <name…>` and `cargo xtask fetch-models` fill `.natives/`; `cargo xtask gen` writes schemas and TypeScript bindings; `cargo xtask arch-check` checks layering.
- Lint is `-D warnings`, clippy pedantic, unsafe forbidden; also `cargo machete` and `cargo deny check`.
- UI: `npm run build`/`lint` in `ui/`; design tokens only (a raw colour fails the lint).
- End-to-end: `node e2e/<suite>.mjs <reader.exe> <scratch> <out>`; suites `reader annotations screens intel library large perf explain`.
- Test runs of the app set `EP_DATA_DIR` (own data, no single-instance).
- best: recipes are complete, proven and followed — `test:practices_are_complete_and_proven`
- best: the map stays true: paths, connections and tests it names exist — `test:project_map_is_current`
- best: run the checks of changed topics only (`cargo xtask topics --changed`); all of them before a push — `test:only_the_topics_of_changed_files_are_checked`
check: `cargo test -p xtask`

## #git Commits and history
tags: commit, push, hook, ci
connects: #start #commands
practice: release
- Remote `mtahator/electrolab`, branch `main`. Never skip the hooks (`--no-verify`).
- CI: Windows full, Ubuntu and macOS compile + core tests. A job "not acquired by a runner" is capacity: re-run it.
- best: an installer comes only from committed code that passed every check — `test:a_release_stops_at_the_first_failure` `test:every_suite_is_part_of_a_release`
- best: the product name lives in one place — `test:slug_and_templates`
check: `cargo deny check` · `cargo xtask practices`

## #bus Commands, events, streaming
tags: bus, command, event, hub, cancel, progress, allowed-commands
connects: #inference #explain #search
practice: command
- A command is a typed handler (`foundation/bus/src/bus.rs`); partial results are hub events with the caller's id (`document.searching`, `inference.token`, `understanding.explaining`).
- The window may call only what `hosts/reader/src-tauri/allowed-commands.json` lists. Forgetting it is the usual "works in tests, fails in the app".
- A call can carry its own id so the window can cancel it (`bus_cancel`).
- best: a cancelled call ends and says so — `test:cancel_stops_the_answer`
check: `cargo test -p ep-bus`

## #pdfium PDF engine and lanes
tags: pdfium, render, lane, priority, background, worker, slow
connects: #text #viewer #search #ocr
practice: engine fact longwork
- One PDFium worker thread per process, shared by every `PdfiumBackend` (two starts crashed). Two queues: interactive first, background second (`engines/document/adapters/pdfium/src/lib.rs`).
- Background work runs inside `ep_document_core::pdf::background(|| …)`.
- Cheap `sizes()` for layout; per-page `page_geom()`; the whole-document geometry is read in chunks of 4 pages.
- Why: previews and preparation once made a 759-page book wait 16–28 s. Rules R1–R8: `docs/design/review-2026-11-15-large-documents.md`.
- best: what the reader waits for goes before background work — `test:interactive_requests_go_before_background_work`
- best: bad files give typed errors, never a crash — `test:bad_input_is_typed_not_a_crash`
check: `cargo test -p ep-document-adapter-pdfium`

## #pages Page sizes and directions
tags: page size, landscape, portrait, rotate, mediabox, cropbox, inherited, squeezed, stretched, mixed
connects: #pdfium #viewer #text #annotations #print
practice: fact panel
- One source: PDFium's own page box (`geom` in `engines/document/adapters/pdfium/src/lib.rs`), which applies crop box and inheritance; the fast sizes come from the same box. Drawing on the screen: `ui/reader/src/viewer/layout.ts`.
- Was wrong until 2026-11-24: a box inherited from the document was replaced by the turned size and turned again, only after preparation, so landscape pages were squeezed on reopening.
- best: two ways to get a fact are tested against each other, on every kind of input — `test:quick_sizes_and_full_geometry_agree`
- best: a fact does not change when background work finishes — `test:page_sizes_stay_the_same_after_preparation`
- best: a part that shows one document starts anew for another one — `test:thumbnails_keep_the_shape`
- best: a page's box, drawing and thumbnail have one shape, also in split view and after reopening — `test:pages_keep_their_own_shape`
check: `cargo test -p ep-document-adapter-pdfium` · `node e2e/pages.mjs <reader.exe> <scratch> <out>`

## #text Page text and preparation
tags: text, analysis, preparation, stages, cache, single flight
connects: #pdfium #search #ocr #labels #equations
practice: fact longwork
- One text extraction per page shared by search, preparation and the text layer; one analysis per file at a time (`engines/document/facade/src/intel.rs`).
- Stages: text → OCR → outline → objects → links → equations. "Prepared" is published before the slow equations stage too.
- Bump `INTEL_IMPL` when results change, or old caches are served.
- best: text can be selected on a large book at once — `test:text_selectable`
- best: documents that are not papers get nothing academic invented — `test:non_academic_documents`
check: `cargo test -p ep-document --test intel`

## #search Search and the find bar
tags: search, find, ctrl+f, stream, hits, next, previous
connects: #text #viewer #bus #explain
practice: command longwork panel
- `document.search` streams: 12 pages a batch, nearest page first; a newer search in the same group cancels the old one.
- There is one search: Ctrl+F and the side panel share `ui/reader/src/viewer/find.ts`.
- User rule: results appear at once and the count grows; never wait for the whole run.
- best: hits arrive while the search runs — `test:find_streams`
- best: limits cut hits, never counts — `test:limits_cut_hits_but_not_counts`
check: `cargo test -p ep-document-core search`

## #labels Captions, figure and table numbers
tags: caption, figure 2.3, label, contents, continued
connects: #text #equations #viewer
practice: fact
- The CAPTION rule in `engines/document/core/src/intel/layout.rs` takes multi-level numbers (2.3, A.1, S2, IV), rejects sentences and "(Continued)"; contents merge by label.
- Without the layout model, `caption_area()` finds the picture.
- best: a label keeps its printed number; a sentence is not a caption — `test:labels_keep_the_printed_number_and_sentences_are_not_captions`
check: `cargo test -p ep-document-core intel`

## #equations Equations and LaTeX
tags: equation, latex, formula, pix2tex, katex, refinement
connects: #text #labels #models
practice: fact longwork ai
- Shown as pictures at once; LaTeX-OCR (optional, 170 MB) adds LaTeX in the background: `engines/document/adapters/onnx/src/formula.rs`, ADR `docs/adr/ADR-015-formula-recognition.md`.
- Resampling must match Pillow exactly, or the LaTeX changes.
- Later, not built: a refinement loop with a small vision model (plan P2.EQR).
- best: an equation is shown whole — `test:equations_are_whole`
- best: the LaTeX equals the reference implementation's — `test:same_latex_as_the_reference_on_the_spike_crops`
check: `cargo test -p ep-document-adapter-onnx`

## #ocr Scanned pages
tags: ocr, ppocr, scan, unlimited-ocr, oxr, english
connects: #text #models #natives
practice: fact file
- PP-OCRv5 on ONNX Runtime, shipped, offline, same on every device: `docs/adr/ADR-013-open-offline-ocr.md`. Only English is tested and promised.
- Tried and not used: OXR (`docs/adr/ADR-016-oxr-page-parser.md`), Unlimited-OCR Q4_K_M (`docs/adr/ADR-017-unlimited-ocr.md`).
- Compare another OCR on the same pages: the ignored test `ocr_bench` in `engines/document/facade/tests/intel.rs`.
- best: a scanned page becomes searchable, the scan is kept — `test:ocr_makes_a_scanned_page_searchable`
check: `cargo test -p ep-document --test intel ocr_makes`

## #models Optional models and settings
tags: model, download, settings, install, egret, group
connects: #equations #ocr #inference #natives
practice: file setting
- Page models: `engines/document/facade/src/models.rs` (`group_of`; the host downloads and removes a whole group, pinned by SHA-256).
- Layout (Egret-medium) and LaTeX-OCR are optional: asked by the installer, changed in Settings.
- Language models: #inference.
- best: without an optional model the app still works and offers it — `test:no_model_offer_and_text_links`
check: `node e2e/intel.mjs <reader.exe> <scratch> <out>`

## #natives Native libraries
tags: natives, onnxruntime, nuget, llama.cpp, pdfium, dll
connects: #models #ocr #inference #commands
practice: file release
- `natives.toml` pins URL and SHA-256; `xtask/src/natives.rs` unpacks; a `files` map copies chosen files out of a package.
- ONNX Runtime comes from NuGet: its GitHub releases went offline on 2026-11-17.
- `chrono` (through pdfium-render) is an accepted "unmaintained" advisory in `deny.toml`.
- best: operating system code stays in its adapters — `test:finds_cfg_and_std_os`
check: `cargo test -p xtask` · `cargo xtask fetch-natives pdfium onnxruntime llama-cpp-vulkan`

## #viewer Pages, previews, zoom, motion
tags: viewer, scroll, zoom, pinch, touch, blur, preview, thumb, restore
connects: #pdfium #annotations #search #explain
practice: page longwork fact
- `ui/reader/src/viewer/Viewer.tsx`; previews ±40 pages in the background lane (`previews.ts`), two interactive and two background slots (`queue.ts`).
- Blur only when flying over pages (speed over 100 ms and more than a page); none at rest.
- Zoom keeps the point under the pointer; pinch on touchpad (Ctrl+wheel) and touch. Touch laptops are always supported (design language §17.8).
- `.link-layer` must not take pointer events, or highlighting breaks. Restoring a place waits for the layout (`restoreTo`).
- best: zoom keeps the point under the pointer — `test:wheel_zoom_at_pointer` `test:pinch_zoom_touchpad` `test:pinch_zoom_touch_screen`
- best: blur only in flight — `test:motion_blur_only_in_flight`
- best: nothing covers the text — `test:text_not_covered`
check: `node e2e/large.mjs <reader.exe> <scratch> <out>` · `npm run lint` · `node e2e/reader.mjs <reader.exe> <scratch> <out>` · `node e2e/perf.mjs <reader.exe> <scratch> <out>`

## #selection Selecting and copying text
tags: select, selection, copy, blue, text layer, caret, font
connects: #viewer #annotations #text #explain
practice: fact test
- Invisible words over the drawing (`ui/reader/src/viewer/Page.tsx`) are fitted to the printed word with a horizontal scale (`ui/reader/src/viewer/textfit.ts`); they only find letters under the pointer.
- The blue bands are drawn from the engine's character boxes (`selectionRows` in `ui/reader/src/annotations/geometry.ts`); the browser's own selection colour is off.
- Was wrong until 2026-11-24: interface font never fitted, heights not updated on zoom, copy took whole words.
- best: what is blue is what is copied, letter for letter — `test:blue_is_what_is_copied`
- best: words are fitted to print at every zoom — `test:words_fit_the_print` `test:words_fit_after_zoom`
- best: the pointer finds the printed letter — `test:pointer_finds_the_letter`
check: `node e2e/selection.mjs <reader.exe> <scratch> <out>`

## #annotations Highlights, notes, selection bar
tags: highlight, annotation, comment, selection bar, notes, undo
connects: #viewer #explain
practice: popup data
- Selection as character ranges: `ui/reader/src/annotations/selection.ts`; the bar: `ui/reader/src/annotations/SelectionBar.tsx`; saving into the PDF: ADR `docs/adr/ADR-006-annotation-model-and-writer.md`.
- Notes and "Add to notes" chips: `ui/reader/src/notes/state.ts`.
- best: a mark is stored at once and survives a restart — `test:annotations_after_restart`
- best: a failed edit changes nothing — `test:failed_edit_changes_nothing`
check: `node e2e/annotations.mjs <reader.exe> <scratch> <out>`

## #layout Sizes, dialogs and Settings
tags: layout, dialog, settings, sheet, overflow, spill, width, scroll, text size, small window, panels
connects: #viewer #models #inference #print #library
practice: page popup dialog panel setting
- Rules: design language §5.4 and §9.9; where they come from: `docs/design/review-2026-11-24-layout.md`.
- One `Dialog` in `ui/kit/src/components.tsx`, sizes in `ui/kit/src/kit.css` (440, sheet 640); the body scrolls. Settings: `ui/reader/src/shell/Settings.tsx`.
- The paper keeps 320 px; side panels give way in `ui/reader/src/shell/frame.ts`.
- `ui/kit/scripts/widths.mjs` refuses widths on content (`pnpm lint`); a new surface is added to its list.
- best: the container owns the width, nothing spills — `test:nothing_spills_out_of_its_box`
- best: no control is cut off, nothing leaves the window — `test:no_control_is_cut_off` `test:nothing_leaves_the_window`
- best: controls do not touch — `test:controls_do_not_touch`
- best: the paper keeps its room — `test:paper_keeps_room`
- best: every part is checked, at every size — `test:every_part_opens`
check: `node e2e/layout.mjs <reader.exe> <scratch> <out>` · `npm run lint` · `node e2e/screens.mjs <reader.exe> <scratch> <out>`

## #print Printing
tags: print, ctrl+p, dialog
connects: #viewer #annotations
practice: dialog fact
- `ui/reader/src/shell/Print.tsx`: pages are drawn as images into a print-only layer, then the standard dialog. Never print the window.
- The Windows "print" verb fails (the Reader is the default PDF app and registers only "open").
- More print features are written down (design language §17.9) and not built until asked.
- best: exactly the chosen pages are printed — `test:print_prints_the_chosen_pages`
check: `node e2e/library.mjs <reader.exe> <scratch> <out>`

## #library Library
tags: library, covers, rename, remove, undo
connects: #start #annotations
practice: page data
- `ui/reader/src/shell/LibraryView.tsx`: list and cover views, rename, remove with Undo. Removed papers are purged when the library is next opened (`engines/workspace`).
- best: a removed paper can be brought back until the library is next opened — `test:remove_restore_and_purge_on_next_open`
check: `cargo test -p ep-workspace` · `node e2e/library.mjs <reader.exe> <scratch> <out>`

## #inference Language models (M5)
tags: inference, llm, model folder, search models, suggestion, task, gguf, llama.cpp, sidecar, qwen, replay, schema, vulkan, cuda
connects: #explain #bus #natives #models
practice: engine ai file setting
- Contract `engines/inference/CONTRACT.md`; decision `docs/adr/ADR-005-local-model-runtime.md`. Qwen3-4B Q4_K_M on GPUs ≥ 4 GB, LFM2.5-1.2B Q8 otherwise.
- `llama-server` is a child on 127.0.0.1, random port, one-time key, started through the Job Object spawner so it ends with the app (`engines/inference/adapters/llamacpp/src/lib.rs`; HTTP in `http.rs` beside it).
- Paper text goes only into the labelled DATA section (`prepare` in `engines/inference/core/src/lib.rs`); prefill is a trailing assistant message; a schema is enforced, checked, retried once.
- Models are found by themselves: folders and their sub-folders, each GGUF recognised by its header (`engines/inference/core/src/gguf.rs`, `engines/inference/facade/src/catalog.rs`); one is suggested, the user chooses per task in `ui/reader/src/shell/LanguageModels.tsx`. Choices: `llm.json` in the app's models folder.
- `Replay` answers recorded tasks in tests and fails on unknown ones. Live test: set `EP_LLM_DIR` to a folder with a known GGUF.
- best: paper text is data, never instructions, and cannot close its own section — `test:data_is_a_labelled_section_after_the_question`
- best: structured output is valid or the call fails — `test:schema_is_retried_once_then_fails`
- best: local-only text never goes to a remote backend — `test:local_only_never_goes_to_a_remote_backend`
- best: the runtime listens on this machine only, with its own key — `test:the_sidecar_is_loopback_only_with_its_own_key`
- best: a dead runtime is restarted once, then the call fails — `test:a_dead_sidecar_is_restarted_once`
- best: its output is read defensively; letters split between reads stay whole — `test:sse_lines_are_read_defensively` `test:letters_split_between_reads_stay_whole`
- best: the state can be asked while a model runs — `test:models_and_status_answer_while_a_model_runs`
- best: models are found wherever they are in the folder, by what they are — `test:models_in_sub_folders_are_found_by_what_they_are` `test:folder_is_searched_with_its_sub_folders`
- best: a file from outside is untrusted: broken headers are errors, never crashes — `test:broken_headers_are_errors_never_crashes`
- best: the suggestion fits the machine; a choice is kept, and a lost one says so — `test:the_suggestion_fits_the_machine` `test:a_task_keeps_its_model_after_a_restart` `test:a_chosen_model_that_is_gone_falls_back_and_says_so`
- best: tests never reach a real model by accident — `test:replay_answers_recorded_tasks_and_refuses_others`
check: `cargo test -p ep-inference-core -p ep-inference-contract -p ep-inference-adapter-llamacpp -p ep-inference` · `node e2e/models.mjs <reader.exe> <scratch> <out>`

## #explain Explain a selection (M5)
tags: explain, understanding, draft, check, evidence, double-click, card
connects: #inference #search #text #viewer #annotations
practice: ai popup command
- Contract `engines/understanding/CONTRACT.md`; code `engines/understanding/facade/src/lib.rs`: streamed draft while the paper is searched, then a check that says OK or corrects. Prompts are the measured ones of spike S3.
- Status is honest: checked, corrected or unchecked. Quotes are kept only when verbatim on their page.
- Interface rules: design language §11.
- best: a wrong draft is corrected from the paper; the status is never better than the check — `test:a_wrong_draft_is_corrected_from_the_paper` `test:no_passages_leaves_it_unchecked` `test:a_vague_check_leaves_it_unchecked`
- best: every quote stands on its page, word for word — `test:quotes_are_verbatim`
- best: Stop stops the model, not only the card — `test:cancelling_explain_cancels_the_model` `test:stop_ends_the_answer`
- best: no model: a double-click stays quiet, the button explains — `test:no_model_is_quiet_then_clear`
- best: the card never covers its word — `test:card_does_not_cover_the_word`
- best: the model process ends with the app — `test:model_process_ends_with_the_app`
check: `cargo test -p ep-understanding` · `node e2e/explain.mjs <reader.exe> <scratch> <out>`

## #pitfalls Things that cost time
tags: pitfall, bug, lesson, gotcha
connects: #start #commands
practice: bug test
- A lock guard inside an `if let` condition lives through the block: clone first, then await. Two `.lock()` of one lock in one statement wait for each other forever.
- Python heredocs turn `\r\n` into real line breaks in Rust byte strings.
- `ref` on an SVG child is not bound in Solid; find it by id. Compare zoom by value, not by object.
- Timings measured while compiling, or while other programs use the CPU, are not results.
- A dry run may skip a step only if that step is checked elsewhere on the real path (the print failure).
- best: a lesson becomes a test where it can — `test:chunked_body_split_anywhere`
check: `cargo test -p xtask`
