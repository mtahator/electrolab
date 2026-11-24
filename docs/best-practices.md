# Best practices, by kind of work

What to do **every time** something of this kind is made or changed. One recipe per kind; `docs/project-map.md` says which recipes a topic needs (`practice:`). `cargo xtask practices` checks that every recipe is complete and that every test it names exists; CI and `cargo xtask release` refuse a build when it fails.

- `cargo xtask practices` lists the recipes and checks them; `cargo xtask practices popup` prints one.
- A recipe has **read** (first), **do**, **never**, **proof** (tests that hold it) and **done** (what is run).
- A founder report or a bug that a recipe should have prevented changes the recipe in the same commit (see *bug*).

---

## page A new page or view
tags: page, screen, view, tab, library
- read: design language §2 (frame), §5.4 (boxes), §8 (components), §12 (words); the topics it touches in the project map
- do: build it from kit components; give it empty, loading, error and offline states; every text through `t()`
- do: show something within 100 ms and fill it as results come; say what is happening when it takes longer
- do: give every action a name, a tooltip, a key and a place in the command palette; Esc leads back
- do: keep what was found or written when the person goes elsewhere and comes back (state outside the view)
- do: add it to `e2e/layout.mjs` (name under PARTS, file under `covers:`)
- never: a width or a colour on content; work on the interactive path that could be in the background
- proof: `test:every_part_opens` `test:nothing_spills_out_of_its_box` `test:paper_keeps_room` `test:coming_back_keeps_what_was_found`
- done: `node e2e/layout.mjs`, `node e2e/screens.mjs`, the page's own suite

## popup A pop-up: card, popover, menu or toolbar
tags: popup, popover, card, menu, tooltip, selection bar
- read: design language §8 (Popover, Card, Menu), §11 (AI cards), §17.7 (transient things)
- do: open next to its anchor without covering it, inside the window on every side; one of a kind at a time
- do: close on Esc and on a click elsewhere; a hover pop-up also on scroll and on leaving; give the focus back
- do: work with touch (a tap opens what a hover opens) and with the keyboard
- do: stop the work it started when it closes
- never: open by itself; change the zoom or the place in the paper; a layer over the page that takes the pointer
- proof: `test:nothing_leaves_the_window` `test:card_does_not_cover_the_word` `test:esc_closes` `test:hover_popover_closes_on_scroll` `test:text_not_covered`
- done: `node e2e/layout.mjs`, the suite of the part it belongs to

## dialog A dialog or a sheet
tags: dialog, sheet, settings, form, confirm
- read: design language §9.9, §9.8 (undo instead of confirm), §5.4
- do: use the kit's `Dialog`: 440 px to ask one thing, `size="lg"` for settings, lists and forms
- do: take what is needed out of the dialog's state before closing it (after closing, its state is gone)
- do: press every button of it in a test, the confirming one too
- do: name the buttons with verbs; first focus on the safe or main one; Esc cancels
- do: keep rows 8 px apart; let a row's control wrap under its text
- never: content with a width of its own; a dialog where undo is possible; a dialog that cannot be closed
- proof: `test:nothing_spills_out_of_its_box` `test:no_control_is_cut_off` `test:controls_do_not_touch` `test:deleting_a_folder_keeps_the_papers`
- done: `node e2e/layout.mjs`, the suite that presses its buttons

## panel A side panel or a list in it
tags: panel, sidebar, list, thumbnails, contents
- read: design language §2 (panels, room for the paper), §10.4 (lists)
- do: start anew when another document is shown: state, pictures and observers are keyed by the document
- do: draw only what is near the view; keep the shape of what will come (no jumps)
- do: remember width and choice; leave the paper its 320 px
- never: state of one document shown for another
- proof: `test:thumbnails_keep_the_shape` `test:paper_keeps_room`
- done: `node e2e/layout.mjs`, `node e2e/pages.mjs`

## setting A setting
tags: setting, preference, option
- read: design language §9.9; the contract of the engine it belongs to
- do: say what it does in plain words; show its effect at once; keep it over a restart
- do: give it a good default, so that nobody has to set it; find by machine what can be found (files, models, sizes)
- do: say what was found, what cannot be used and why
- never: ask for an exact file name or folder level; a choice that silently does nothing
- proof: `test:choices_survive_a_restart` `test:the_order_can_be_changed` `test:folder_is_searched_with_its_sub_folders` `test:models_are_listed_and_suggested`
- done: `node e2e/layout.mjs`, the suite of the setting

## command A command of an engine
tags: command, bus, contract, api, allowed
- read: programming guideline Part 2; the engine's CONTRACT.md
- do: contract first (input, output, errors, events, guarantees, changelog), then schema (`cargo xtask gen`), then code
- do: make it cancellable; stream when it takes longer than 100 ms; check every input at the border
- do: list it in `allowed-commands.json` only when the window needs it
- never: an untyped error; blocking work on the async threads; a lock held over an await
- proof: `test:cancel_stops_the_answer` `test:what_cannot_work_is_refused` `test:errors_follow_the_contract`
- done: the engine's tests, `cargo xtask schema-check`, `cargo xtask arch-check`

## engine An engine or an adapter
tags: engine, crate, adapter, port
- read: programming guideline Part 1; `cargo xtask arch-check` (layers)
- do: contract, core (pure), adapters (I/O), facade (bus); a fake or Replay of every port before the real one
- do: keep the operating system behind `ep-os`
- never: another engine's core or facade; a test that needs the network, a GPU or a private file to pass
- proof: `test:breaking_the_engine_order_fails` `test:finds_cfg_and_std_os` `test:replay_answers_recorded_tasks_and_refuses_others`
- done: `cargo xtask arch-check`, the engine's conformance suite

## fact Something read from a document
tags: page size, text, label, geometry, pdf, extraction
- read: ADR-003; the topics #pages, #text, #labels
- do: one source for one fact; when there are two ways to get it, test them against each other on every fixture
- do: make a fixture for every real file that broke it (written by us, never the user's file)
- do: cover turned pages, inherited boxes, crop boxes, landscape, odd sizes, empty and damaged files
- never: a fact that changes when background work finishes
- proof: `test:quick_sizes_and_full_geometry_agree` `test:page_sizes_stay_the_same_after_preparation` `test:damaged_and_empty_files_give_typed_errors`
- done: `cargo test -p ep-document-adapter-pdfium -p ep-document`, `node e2e/pages.mjs`

## longwork Something that takes time
tags: search, background, stream, progress, large document
- read: design language §17.1–17.3; programming guideline 4.1a
- do: first result at once, nearest to the reader first, then the rest; a newer request ends the older one
- do: background work in the background lane, in pieces, cancellable; one run for one result
- do: work that starts by itself is kept piece by piece, goes on where it stopped, waits on a low battery, and shows one quiet line
- do: limit depth, number and time, and say so when a limit was reached
- never: wait for everything; the same work twice; small reads or seeks by the hundred thousand
- proof: `test:find_streams` `test:interactive_requests_go_before_background_work` `test:real_models_are_found_below_the_folder` `test:a_paper_closed_too_early_is_gone_on_with_the_next_time` `test:reading_stays_fluid`
- done: `node e2e/large.mjs` with the budgets of §17.3

## ai A feature with a language model
tags: ai, llm, model, explain, prompt
- read: design language §11; `engines/inference/CONTRACT.md`
- do: on request only; paper text only in the data section; facts checked by code; status in words
- do: Stop ends the model, not only the card; a Replay test for every path
- do: where a model ranks or chooses, add what code can count (words, numbers, kinds); show what code alone finds first
- do: work without a model as far as possible, and say so; background answers wait for people
- do: try it with as many papers as a person has (40 and more), and with another program holding the graphics memory
- do: one model, kept loaded; measure the free memory before loading another; a second choice for when it does not fit
- do: say a failure in plain words with Try again; a part that fails does not lose the rest
- do: look at real output with a real paper before the page is built, and again in the finished page
- never: text from a paper in an instruction; local-only text to a remote backend; "checked" without a check
- proof: `test:data_is_a_labelled_section_after_the_question` `test:local_only_never_goes_to_a_remote_backend` `test:a_wrong_draft_is_corrected_from_the_paper` `test:stop_ends_the_answer` `test:what_a_person_asks_goes_before_background_answers` `test:finding_works_without_a_model` `test:a_full_memory_means_the_next_model` `test:all_papers_of_a_library_are_sorted` `test:a_part_that_cannot_be_read_does_not_lose_the_others`
- done: the engine's tests, `node e2e/explain.mjs` with and without a model

## file A file or program from outside
tags: download, model, native, gguf, pdf, licence, security
- read: the topics #natives and #models; programming guideline Part 6
- do: pin address and SHA-256 of what is shipped or downloaded; keep its licence text with the app
- do: treat every file as untrusted: check lengths, limit sizes, read headers before contents
- do: start other programs on this machine only, with a one-time key, tied to the app's life
- never: trust a file name to say what a file is
- proof: `test:broken_headers_are_errors_never_crashes` `test:the_sidecar_is_loopback_only_with_its_own_key` `test:model_process_ends_with_the_app`
- done: `cargo deny check`, the engine's tests

## data Something kept on disk
tags: migration, storage, library, settings file
- read: ADR-010; `engines/workspace/CONTRACT.md`
- do: write to a new file and rename; numbered migrations; take over what an older version wrote
- do: undo instead of asking; delete for good only later
- do: what is hashed is read back exactly as it was written (numbers too); a large input is logged by size and hash
- do: damage of a log or a cache never locks a person out: keep it aside, begin a new one, say so in the app's log
- never: change the user's own files or folders without being asked
- proof: `test:any_number_survives_being_written_and_read` `test:a_broken_log_is_kept_aside_and_work_goes_on` `test:large_inputs_are_recorded_by_size_and_hash` `test:remove_restore_and_purge_on_next_open` `test:the_order_is_kept_and_can_be_automatic_again` `test:forgetting_a_folder_touches_nothing_on_disk`
- done: the engine's tests, restart in the end-to-end suite

## test A test
tags: test, e2e, check
- read: programming guideline 5.4
- do: see it fail once, on the version that has the bug; then see it pass
- do: use real input (mouse, keys, wheel, touch) and the built app; measure what the user sees
- do: check that the step happened (the zoom changed, the page was reached) before checking its result
- do: begin clean: absolute folders of its own, cleared before the run; nothing is remembered from the last one
- never: a timing taken while compiling; a longer wait instead of a cause; a test that cannot fail
- proof: `test:words_fit_after_zoom` `test:project_map_is_current`
- done: the checks of the changed topics (`cargo xtask topics --changed`)

## bug A bug or a founder report
tags: bug, report, fix, root cause
- read: the topic in the project map; the document that should have prevented it
- do: reproduce it; find the cause in the code **and** the document or rule that allowed it
- do: look for the same mistake everywhere else; fix all places
- do: rule and recipe first, then a test that fails, then the fix, then the map
- never: a fix without a test; a rule without a check
- proof: `test:practices_are_complete_and_proven` `test:project_map_is_current`
- done: `cargo xtask topics --changed` and its checks, `cargo xtask practices`

## release A release (installer)
tags: release, deploy, installer, ci, push
- read: CONTRIBUTING.md
- do: `cargo xtask release`: it runs every check below, in order, stops at the first failure and builds the installer only after the last one
- do: commit first; the report names the commit and the installer's SHA-256
- never: an installer from uncommitted code; skip a check or a hook
- proof: `test:a_release_stops_at_the_first_failure` `test:every_suite_is_part_of_a_release`
- done: format, lint, architecture, schemas, practices, licences, unused dependencies, all tests, interface checks, build, every end-to-end suite, installer
