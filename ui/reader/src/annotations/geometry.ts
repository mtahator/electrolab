// Pure helpers for annotations (no DOM, no Solid): quads from the engine's character boxes,
// stroke simplification, hit testing, and citation text. Tested with `node --test`.

import type { Annotation, PagePoint, PageQuad, PageRect, PageText } from "@electropaper/bindings";

/** Characters `start..end` of a page, as one quad per line piece (P1.M3.UI.2: marks follow the
 *  engine's character boxes, not the DOM selection rectangles). */
export function quadsFor(pt: PageText, start: number, end: number): PageQuad[] {
  const chars = Array.from(pt.text);
  const out: PageQuad[] = [];
  const lines = pt.lines.length > 0 ? pt.lines : [{ start: 0, end: chars.length, rect: { page: pt.page, x0: 0, y0: 0, x1: 0, y1: 0 } }];
  for (const line of lines) {
    const s = Math.max(start, line.start);
    const e = Math.min(end, line.end);
    if (s >= e) continue;
    let r: [number, number, number, number] | null = null;
    for (let i = s; i < e; i++) {
      const b = pt.boxes[i];
      const c = chars[i] ?? "";
      if (!b || b.length < 4 || /\s/.test(c) || c === "­") continue;
      const [x0, y0, x1, y1] = b as [number, number, number, number];
      r = r ? [Math.min(r[0], x0), Math.min(r[1], y0), Math.max(r[2], x1), Math.max(r[3], y1)] : [x0, y0, x1, y1];
    }
    if (!r) continue;
    const [x0, y0, x1, y1] = r;
    out.push({
      page: pt.page,
      p: [
        { x: x0, y: y0 },
        { x: x1, y: y0 },
        { x: x0, y: y1 },
        { x: x1, y: y1 },
      ],
    });
  }
  return out;
}

/** Bands that show characters `start..end` as selected: one per line piece, as high as the line,
 *  from the first to the last selected character with the spaces between (design language §9.6). */
export function selectionRows(pt: PageText, start: number, end: number): PageRect[] {
  const out: PageRect[] = [];
  for (const q of quadsFor(pt, start, end)) {
    const [a, , , d] = q.p;
    if (!a || !d) continue;
    const mid = (a.y + d.y) / 2;
    const line = pt.lines.find((l) => l.rect.y0 <= mid && l.rect.y1 >= mid && l.rect.x0 <= a.x + 1 && l.rect.x1 >= d.x - 1 && l.start < end && l.end > start);
    out.push({ page: pt.page, x0: a.x, y0: line ? Math.min(line.rect.y0, a.y) : a.y, x1: d.x, y1: line ? Math.max(line.rect.y1, d.y) : d.y });
  }
  return out;
}

/** The text of characters `start..end`, as the copy command gives it (soft hyphens at line ends
 *  joined, other line breaks become spaces). */
export function textFor(pt: PageText, start: number, end: number): string {
  return clean(Array.from(pt.text).slice(start, end).join(""));
}

export function clean(s: string): string {
  return s
    .replace(/­\n/g, "")
    .replace(/[ \t]*\n[ \t]*/g, " ")
    .replace(/­/g, "")
    .trim();
}

/** Character index in a word span for a DOM offset in its shown text (the shown text drops the
 *  word's first soft hyphen and counts UTF-16 units). */
export function charIndex(original: string, shownOffset: number): number {
  const chars = Array.from(original);
  const hy = chars.indexOf("­");
  let units = 0;
  for (let i = 0; i < chars.length; i++) {
    if (i === hy) continue;
    if (units >= shownOffset) return i;
    units += (chars[i] ?? "").length;
  }
  return chars.length;
}

/** Ramer–Douglas–Peucker: drop points closer than `eps` to the simplified line. */
export function simplify(points: PagePoint[], eps = 0.6): PagePoint[] {
  if (points.length < 3) return points.slice();
  const dist = (p: PagePoint, a: PagePoint, b: PagePoint): number => {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy);
    if (len === 0) return Math.hypot(p.x - a.x, p.y - a.y);
    return Math.abs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x) / len;
  };
  const keep = new Array<boolean>(points.length).fill(false);
  keep[0] = true;
  keep[points.length - 1] = true;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length > 0) {
    const [lo, hi] = stack.pop() as [number, number];
    let best = -1;
    let bestD = eps;
    for (let i = lo + 1; i < hi; i++) {
      const d = dist(points[i] as PagePoint, points[lo] as PagePoint, points[hi] as PagePoint);
      if (d > bestD) {
        best = i;
        bestD = d;
      }
    }
    if (best >= 0) {
      keep[best] = true;
      stack.push([lo, best], [best, hi]);
    }
  }
  return points.filter((_, i) => keep[i]);
}

function inRect(r: PageRect, x: number, y: number, pad = 0): boolean {
  return x >= r.x0 - pad && x <= r.x1 + pad && y >= r.y0 - pad && y <= r.y1 + pad;
}

function segDist(p: PagePoint, a: PagePoint, b: PagePoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const l2 = dx * dx + dy * dy;
  const t = l2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** Whether a page-space point touches an annotation (`tol` in points). */
export function hits(a: Annotation, x: number, y: number, tol = 3): boolean {
  const p = { x, y };
  switch (a.kind) {
    case "highlight":
    case "underline":
    case "strike_out":
    case "squiggly":
      return (a.quads ?? []).some((q) => {
        const xs = q.p.map((c) => c.x);
        const ys = q.p.map((c) => c.y);
        return inRect({ page: q.page, x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) }, x, y, 1);
      });
    case "ink":
      return (a.paths ?? []).some((s) => s.some((pt, i) => (i === 0 ? Math.hypot(pt.x - x, pt.y - y) <= tol + (a.width ?? 1) : segDist(p, s[i - 1] as PagePoint, pt) <= tol + (a.width ?? 1) / 2)));
    case "line":
    case "arrow": {
      const l = a.line;
      return !!l && l.length === 2 && segDist(p, l[0] as PagePoint, l[1] as PagePoint) <= tol + (a.width ?? 1) / 2;
    }
    case "rectangle": {
      const r = a.rect;
      const near = (v: number, e: number): boolean => Math.abs(v - e) <= tol + (a.width ?? 1) / 2;
      return inRect(r, x, y, tol) && (near(x, r.x0) || near(x, r.x1) || near(y, r.y0) || near(y, r.y1));
    }
    case "ellipse": {
      const r = a.rect;
      const cx = (r.x0 + r.x1) / 2;
      const cy = (r.y0 + r.y1) / 2;
      const rx = Math.max(0.1, (r.x1 - r.x0) / 2);
      const ry = Math.max(0.1, (r.y1 - r.y0) / 2);
      const k = Math.hypot((x - cx) / rx, (y - cy) / ry);
      return Math.abs(k - 1) * Math.min(rx, ry) <= tol + (a.width ?? 1) / 2;
    }
    default:
      return inRect(a.rect, x, y);
  }
}

/** Short author list for citations: "Lovelace", "Lovelace and Babbage", "Lovelace et al.". */
export function citeAuthors(authors: string[]): string {
  const last = (n: string): string => {
    const t = n.trim();
    if (t.includes(",")) return t.split(",")[0]?.trim() ?? t;
    const parts = t.split(/\s+/);
    return parts[parts.length - 1] ?? t;
  };
  if (authors.length === 0) return "";
  if (authors.length === 1) return last(authors[0] as string);
  if (authors.length === 2) return `${last(authors[0] as string)} and ${last(authors[1] as string)}`;
  return `${last(authors[0] as string)} et al.`;
}

/** Quoted text with its source and page (P1.M3.UI.8): “text” (Lovelace et al., p. 3). */
export function citation(text: string, paper: { title?: string | null; authors?: string[]; original_name: string }, page: number): string {
  const who = citeAuthors(paper.authors ?? []) || paper.title || paper.original_name.replace(/\.pdf$/i, "");
  return `“${clean(text)}” (${who}, p. ${page})`;
}

/** A new annotation id. */
export function newId(): string {
  const c = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  const r = c?.randomUUID ? c.randomUUID() : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  return `ep-${r}`;
}

/** Bounds of points, padded. */
export function boundsOf(page: number, pts: PagePoint[], pad = 0): PageRect {
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  return { page, x0: Math.min(...xs) - pad, y0: Math.min(...ys) - pad, x1: Math.max(...xs) + pad, y1: Math.max(...ys) + pad };
}
