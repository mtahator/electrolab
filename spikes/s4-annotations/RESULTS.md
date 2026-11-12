# Spike P0.S4 — Results

Date: 2026-11-12 · PDFium chromium/8125 · lopdf 0.46.0 · test file: page 1 of fixture `arxiv-2111.12066v2` (two-column, 1.66 MB) · outputs in `out/`

## Verdict: **pass, with our own writer** — annotations are written by the Document engine itself as an incremental update (lopdf); PDFium reads and renders them. PDFium cannot be the writer.

| Criterion (phase-0 plan) | Result | Status |
|---|---|---|
| Highlight, underline, strike-out, note with reply, free text, ink, rectangle, arrow — written | All 8, plus the note's popup, a reply (`/IRT`) and a resolved status (`/StateModel Review`, `/State Completed`), each with our own appearance stream, id (`/NM`), author (`/T`), category (`/Subj`) and contents (UTF-16 for Persian) | **pass** |
| Incremental save; original byte-identical | The annotated file is the **original 1,658,852 bytes followed by a 7,103-byte update**; the original file's hash is unchanged | **pass** |
| Read correctly by other readers | **MuPDF**: no repair, no warnings; reply link, status, arrowhead (`OpenArrow`), appearance present for every mark. **PDF.js**: all 10 marks + popup, replies linked to the note. **PDFium** (our reader): all marks with id, author, contents incl. Persian | **pass** |
| Display correctly | Rendered with vs without annotations, every mark changes its pixels in **PDFium** (Chrome's engine) and **MuPDF**. **Edge** (Adobe's Acrobat PDF engine since 2024): highlight, underline, strike-out, arrow, rectangle and our note icon shown correctly (screenshot of the top of page 1; free text and ink below the fold not captured) | **pass** for what was seen |
| Marks made by another tool are read back | 9 marks written by MuPDF's own annotation writer (incl. a reply and an arrow) are all read by PDFium with type, author and contents (Persian included); PDF.js sees the reply link | **pass** |
| Annotations made **in Acrobat** read back; check in **Acrobat desktop** | Acrobat is not installed on this PC | **open — human step** |

## Why PDFium cannot be the writer

1. PDFium's `FPDFAnnot_IsSupportedSubtype`: it can create Text, FreeText, Square, Circle, Highlight, Underline, Squiggly, StrikeOut, Stamp, Ink, Popup — **not Line** (no arrows).
2. PDFium has `FPDFAnnot_GetLinkedAnnot` and `FPDFAnnot_GetLine` but **no setter** for a reply link (`/IRT`) or line coordinates.
3. `pdfium-render` saves with flags 0 only and keeps the document handle private, so **no incremental save** without forking it.

## Findings for the Document engine (Phase 1, M3)

1. **Writer = Document-engine adapter on lopdf `IncrementalDocument`**: clone the page dictionary into the new revision, append annotation objects and appearance streams, rewrite `/Annots` (inline array or referenced array — both occur), save. Pure Rust, MIT, no C code.
2. **Always write appearance streams** (`/AP /N`), with BBox = Rect so content uses page coordinates. Highlight uses an ExtGState with `/BM /Multiply` so the text stays readable. Viewers may still substitute their own icon for notes (MuPDF did; Edge used ours).
3. **Free text in Persian needs an embedded font.** The spike's free-text appearance uses Helvetica (WinAnsi), which cannot draw Arabic script; contents strings are fine (UTF-16), the drawn appearance is not. The writer must embed a subset of a free Unicode font (e.g. Vazirmatn or Noto Sans Arabic, both OFL) when the text is not Latin-1, and shape RTL text before drawing.
4. **Reply and status are separate `Text` annotations** with `/IRT` → parent; status uses `/StateModel (Review) /State (Completed)`. Without their own appearance, PDFium draws a default icon on top of the parent (harmless, same position); decide whether replies get `/F` hidden-but-listed flags after the Acrobat check.
5. **Acrobat adds comment markers** next to any markup that has `/Contents` (seen in Edge). Expected behaviour; the Reader should do the same for consistency.
6. **Ids survive round-trips**: `/NM` (`ep-…`) is read back by PDFium and MuPDF; this is how re-import updates instead of duplicating (P1.M3.PDF.1).
7. **Positions come from PDFium word boxes** (tight bounds, user space) and land exactly on the words in every renderer, so the coordinate rule of ADR-003 holds for writing too.
8. **Third-party files carry their own conventions** (MuPDF adds a `Popup` per note and reply). The reader must keep unknown keys and objects untouched on save — the incremental update never rewrites them.

## Not covered

- Acrobat desktop and Acrobat-made annotations (needs Acrobat; human step before Reader 1.0, P1.M3.PDF.4).
- Rotated pages and pages with a non-zero crop box (add to the round-trip suite with the derived fixtures).
- Squiggly, circle, stamp: trivial variants of tested kinds; covered by the suite in M3.
- Many revisions in a row (save, annotate, save again) and file-size growth over time.
