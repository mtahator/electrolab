# Knowledge engine — contract

Version: 0.1.0      Stability: experimental      Tier: T1
Crates: `ep-knowledge-contract` (types), `ep-knowledge-core` (splitting, checks, ranking — pure), `ep-knowledge` (facade)

## Purpose
Know what is in the library and find it again (Draft 2.2 §14.3). First slice (founder request, 2026-11-24): a **digest** of every paper, kept with it, and **asking the library**: a question or a description in, the papers it can be in and the nearest places out. It uses the Document, Inference and Workspace engines through their contracts; summaries, search, pictures and storage are theirs.

## Commands

| Command | Input → Output | Permission |
|---|---|---|
| `knowledge.digest_paper` | `{ paper, force }` → `Digest` (made when missing or old; resumes where it stopped) | create |
| `knowledge.digests` | `{ papers? }` → `[DigestInfo { paper, level, summary, topics, kind, sections, made_ms, model? }]` (what there is; nothing is made) | read |
| `knowledge.get_digest` | `{ paper }` → `Digest` or `null` | read |
| `knowledge.digest_library` | `{ full, papers?, force }` → `DigestStatus` (starts in the background: first the basic digest of every paper, then the written ones of `papers` in the order asked — also while it works — and, with `full`, of all others, one paper after the other; `force` makes those of `papers` anew) | create |
| `knowledge.digest_stop` | `null` → `DigestStatus` (what is made stays; starting again goes on) | read |
| `knowledge.digest_status` | `null` → `DigestStatus { running, full, done, total, paper?, next, current?, step?, error? }` | read |
| `knowledge.find` | `FindRequest { question, papers?, stream?: { id } }` → `FindResult` (streamed when `stream` is set) | read |

Error kinds: `nothing_to_find`, `no_text` (a paper without any text), and those of the engines used.

## The digest (kept with the paper)
`Digest { version, paper, pages, level: basic | full, title, kind, summary, topics, questions, sections, objects, model?, made_ms, complete }`
- **Where:** `workspace.save_digest` writes `digests/<paper id>.json` in the library folder, next to the managed copy; it goes when the paper is purged. Readable by people and by other programs.
- **Section by section:** `SectionDigest { id, title, level, pages: [from, to], role, summary, tags, terms, support, source: written | quoted | none }`, in the order of the document. Sections come from the PDF's own outline, else from its headings, else from pages (about 3 500 characters each). Long sections are split, short ones joined to the next; at most 80 (deeper levels are joined first). Running heads and feet are left out. A section without a telling heading takes the role of the heading above it; lists of references are recognised without a heading too.
- **Clean tags:** lower case, 1–3 words, singular where that is safe, no doubles, at most 6. `role` is one of: introduction, background, method, data, results, discussion, conclusion, instructions, legal, administrative, appendix, references, other.
- **`terms`** stand in the section word for word; **`support`** are whole sentences of it, with their page, copied by code from the numbers the model names.
- **`objects`**: every labelled figure, table and equation with its page, caption and the words in it (from the Document engine, no model).
- **Level `basic`** (no model, about a second per paper): title, headings, captions, the most telling words. **Level `full`**: written summaries on top.

### How a summary is made right
1. The model sees one section as numbered sentences, in the data section, and answers with an enforced schema.
2. **Numbers:** every number of the summary must stand in the section. If not, it is asked once more with the wrong numbers named; sentences that still hold one are taken out.
3. **Words:** at least 60 % of the summary's content words must stand in the section (word forms counted as the same). If not, or if nothing is left, the summary is **quoted**: the supporting sentences themselves.
4. Terms that do not stand in the section are dropped; tags are cleaned; the sentence numbers are checked.
5. The paper's summary is written from the section summaries only, and passes the same checks against them.
So a summary is never better than its source: it is either checked (`written`), the paper's own words (`quoted`), or absent (`none`).

## Asking the library (`knowledge.find`)
1. **Understand** (model, schema): what is looked for (figure, table, equation, definition, text) and the 1–4 **things** it is about, each with its other names (abbreviation, written-out form, technical wording), in the language of documents. Abbreviations are also written out by code, as the papers do (`NPV` → `net present value`). The question's own words are searched as well (those of another script only where the papers use it). Without a model: every word of the question is a thing.
2. **Related papers:** all digests are ranked by their words (summary, topics, questions, tags, terms, captions, headings); the model chooses among the first 12 and says why in a few words. Without a model: the first 6.
3. **Places:** the words are searched in the related papers (`document.search`), then in the others; captions and section summaries are places too. A place counts by how many different things are named there (by any of their names), then by how rare the words are, and when it is the kind looked for.
4. **Nearest:** the model grades the first 24 places (0–3). What it says, the share of things named and the kind looked for are added up and order them; up to 5 at the top with a grade of 2 or 3 are marked as nearest (of the kind looked for, when there is one). When a figure, table or equation is looked for, all captions of the three most related papers are offered, whatever their words.

Each step is sent as it is there (`knowledge.finding`); the first one holds what the question's own words find, before any model is asked, so something is seen at once. Text that repeats a caption is the same place as the caption.
`FindResult { question, looking_for, terms, related: [Related { paper, why?, score }], places: [Place], others: [{ paper, hits }], used_model, model?, total_ms, digests: { full, basic, total } }`
`Place { paper, page, y, kind: figure | table | equation | section | text, label?, object?, rect?, quote, terms, section?, score, nearest }` — `quote` stands in the paper word for word.

## Events (hub)
- `knowledge.digesting` `DigestStatus`; `knowledge.digested` `{ paper, level }` (also while a written digest grows: what is there can be read).
- `knowledge.finding` `{ id, step: understanding | related | searching | nearest | done, result }`.

## Guarantees
1. The text of papers is data, never instructions; local-only.
2. Numbers in summaries stand in their section; quotes and terms stand in the paper word for word.
3. Background digests wait for what the person asks (`LlmRequest.background`), can be stopped at any time and lose at most one section.
4. Finding works without a model and without written digests (words only), and says so.
5. A question is never sent anywhere; nothing is stored about it.

## Quality
- Conformance with Replay (`engines/knowledge/facade/tests`): wrong numbers asked again and removed, ungrounded summaries quoted, terms and sentences verbatim, resume, stop, streaming order, without a model.
- Live check with the real model on fixture papers (digest accepted share, example question finds the figure).

## Changelog
- 0.1.0 (2026-11-24): digests and asking the library.
