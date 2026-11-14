// The text selection as character ranges of the Document engine's text (not DOM rectangles),
// and text-mark annotations made from it (P1.M3.UI.2, UI.9).

import type { Annotation, AnnotationKind, PageText } from "@electropaper/bindings";

import { boundsOf, charIndex, quadsFor, textFor } from "./geometry";
import { draft } from "./state";

export interface CharRange {
  page: number;
  start: number;
  end: number;
}

/** Character ranges of the current selection, per page, in page order. */
export function selectionRanges(texts: (page: number) => PageText | undefined): CharRange[] {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return [];
  const range = sel.getRangeAt(0);
  const spans = Array.from(document.querySelectorAll<HTMLSpanElement>(".pane.focused .text-layer span[data-page]")).filter((s) => range.intersectsNode(s));
  const byPage = new Map<number, CharRange>();
  for (const s of spans) {
    const page = Number(s.dataset.page);
    let start = Number(s.dataset.start);
    let end = Number(s.dataset.end);
    const pt = texts(page);
    const original = pt ? Array.from(pt.text).slice(start, end).join("") : "";
    if (pt && s.contains(range.startContainer) && range.startContainer.nodeType === Node.TEXT_NODE) {
      start += charIndex(original, range.startOffset);
    }
    if (pt && s.contains(range.endContainer) && range.endContainer.nodeType === Node.TEXT_NODE) {
      end = Number(s.dataset.start) + charIndex(original, range.endOffset);
    }
    if (end <= start) continue;
    const cur = byPage.get(page);
    byPage.set(page, cur ? { page, start: Math.min(cur.start, start), end: Math.max(cur.end, end) } : { page, start, end });
  }
  return [...byPage.values()].sort((a, b) => a.page - b.page);
}

/** One text mark per page for the given ranges (empty when the selection has no character boxes). */
export function marksFor(kind: AnnotationKind, ranges: CharRange[], texts: (page: number) => PageText | undefined, color?: string): Annotation[] {
  const out: Annotation[] = [];
  for (const r of ranges) {
    const pt = texts(r.page);
    if (!pt) continue;
    const quads = quadsFor(pt, r.start, r.end);
    if (quads.length === 0) continue;
    const a = draft(kind, r.page, {
      quads,
      text: textFor(pt, r.start, r.end),
      rect: boundsOf(r.page, quads.flatMap((q) => q.p)),
    });
    if (color) a.color = color;
    out.push(a);
  }
  return out;
}

/** The bounding box of the selection on screen (for placing the selection bar). */
export function selectionBox(): DOMRect | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const rects = Array.from(sel.getRangeAt(0).getClientRects()).filter((r) => r.width > 0 && r.height > 0);
  return rects[rects.length - 1] ?? null;
}
