# ADR-006: The annotation store is the truth; our own incremental writer exports standard PDF annotations

Status: **accepted** — pending one human check in Acrobat desktop before Reader 1.0
Date: 2026-11-12
Plan reference: Draft 2.2 §28.3 (Highlight, Comment, Text on page, Draw, Undo and save), §28.8; Phase 1 M3; task P0.S4

## Context
Reader marks must be fast to edit, never lost, undoable across restarts, and must open correctly in Acrobat and other readers; annotations made elsewhere must be read. The original PDF must never be modified (Draft 2.2 §14.2 guarantee). Spike S4 measured what PDFium can write and tested our own writer.

## Decision
1. **The annotation store (event-sourced, in the project database) is the source of truth.** Editing never touches a PDF.
2. **"Save into PDF" writes a new version of the managed copy as an incremental update**, produced by the Document engine's own writer (adapter on `lopdf::IncrementalDocument`, MIT, pure Rust): original bytes kept as a prefix, new objects and a rewritten `/Annots` appended.
3. **Every annotation is written as a standard subtype with its own appearance stream**: Highlight / Underline / StrikeOut / Squiggly (with `/QuadPoints`), Text (note) + Popup, replies as Text with `/IRT`, resolved state as Text with `/StateModel /State`, FreeText, Ink, Square, Circle, Line with `/LE` arrowheads.
4. **Our id in `/NM`** (round-trip key), category in `/Subj`, author in `/T`, text strings as UTF-16BE when not ASCII.
5. **Non-Latin free text embeds a subset of a free Unicode font** in its appearance (Persian requires shaping); Helvetica is used only for Latin-1 text.
6. **PDFium reads and renders annotations** (ours and other tools'); unknown kinds and keys are kept untouched because the writer only appends.

## Alternatives considered
| Option | Why not |
|---|---|
| PDFium as the writer | Cannot create Line annotations, cannot set `/IRT` reply links, and `pdfium-render` cannot save incrementally |
| Full rewrite of the PDF on save | Breaks signatures, changes bytes the user may rely on, larger risk of damaging odd files |
| Annotations only in our database (no PDF export) | Users must be able to open their marks in Acrobat and share files |
| A commercial PDF SDK | Licence cost and closed source for a problem lopdf solves |

## Consequences
- Annotation export is pure Rust and testable without PDFium; PDFium, MuPDF and PDF.js are used in CI as independent readers of what we write (round-trip suite, P1.M3.PDF.4).
- A small font (subset at export time) ships for non-Latin free text.
- **Open:** a person checks the S4 output (`spikes/s4-annotations/out/annotated.pdf`, regenerate with the README) and a file annotated in Acrobat, in Acrobat desktop, before Reader 1.0.

## Evidence
`spikes/s4-annotations/RESULTS.md`; outputs `out/verify-mupdf.json`, `out/verify-pdfjs.json`, `out/read-pdfium.json`, `out/visible-pdfium.json`.

## Implementation note (2026-11-14, P1.M3)
Built as decided: `ep-document-core::annotations` (validation, edits and inverses, import, write plan), `ep-document-adapter-lopdf` (read `/Annots`, incremental update), facade commands `document.annotations*` and `document.annotate`. Verified: Rust round trips (all kinds, another tool's annotations kept, rotated page, several saves), PDF.js reads the saved file in `e2e/annotations.mjs`. Still open: the Acrobat desktop check (P1.M3.PDF.7) and the Unicode font for non-Latin free text (P1.M3.PDF.5).
