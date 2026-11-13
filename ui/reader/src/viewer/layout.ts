// The one conversion module between page space (PDF points, top-left origin, ADR-003) and screen
// pixels in the scroll area. Nothing else does this arithmetic.

import type { PageSize } from "@electropaper/bindings";

export type ViewMode = "continuous" | "single" | "two_page";

/** How large pages are drawn. */
export type Zoom = { kind: "fit_width" } | { kind: "fit_page" } | { kind: "scale"; factor: number };

export const MIN_SCALE = 0.25;
export const MAX_SCALE = 5;
const GAP = 12;
const MARGIN = 16;

/** Where one page sits in the scroll area (CSS pixels). */
export interface Slot {
  page: number;
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Layout {
  /** CSS pixels per point. */
  scale: number;
  slots: Slot[];
  width: number;
  height: number;
}

/** CSS pixels per point for a zoom in a viewport of `vw` × `vh`. */
export function scaleFor(zoom: Zoom, sizes: PageSize[], mode: ViewMode, vw: number, vh: number): number {
  if (zoom.kind === "scale") return clamp(zoom.factor);
  const widest = Math.max(1, ...sizes.map((s) => s.width));
  const tallest = Math.max(1, ...sizes.map((s) => s.height));
  const across = mode === "two_page" ? 2 : 1;
  const byWidth = (vw - 2 * MARGIN - (across - 1) * GAP) / (widest * across);
  if (zoom.kind === "fit_width") return clamp(byWidth);
  return clamp(Math.min(byWidth, (vh - 2 * MARGIN) / tallest));
}

export function clamp(s: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));
}

/** Positions of every page: known before anything is drawn, so the scroll height never jumps. */
export function layout(sizes: PageSize[], scale: number, mode: ViewMode, vw: number, current: number): Layout {
  const slots: Slot[] = [];
  if (mode === "single") {
    const i = Math.min(Math.max(current, 1), sizes.length) - 1;
    const s = sizes[i] ?? { width: 612, height: 792 };
    const w = s.width * scale;
    const h = s.height * scale;
    const left = Math.max(MARGIN, (vw - w) / 2);
    slots.push({ page: i + 1, left, top: MARGIN, width: w, height: h });
    return { scale, slots, width: Math.max(vw, w + 2 * MARGIN), height: h + 2 * MARGIN };
  }
  let top = MARGIN;
  let widest = 0;
  const step = mode === "two_page" ? 2 : 1;
  for (let i = 0; i < sizes.length; i += step) {
    const row = sizes.slice(i, i + step);
    const rowW = row.reduce((a, s) => a + s.width * scale, 0) + (row.length - 1) * GAP;
    const rowH = Math.max(...row.map((s) => s.height * scale));
    widest = Math.max(widest, rowW);
    let left = Math.max(MARGIN, (vw - rowW) / 2);
    row.forEach((s, k) => {
      slots.push({ page: i + k + 1, left, top, width: s.width * scale, height: s.height * scale });
      left += s.width * scale + GAP;
    });
    top += rowH + GAP;
  }
  return { scale, slots, width: Math.max(vw, widest + 2 * MARGIN), height: top - GAP + MARGIN };
}

/** Slots that intersect the band `[from, to]` (vertical CSS pixels). */
export function visible(l: Layout, from: number, to: number): Slot[] {
  return l.slots.filter((s) => s.top + s.height >= from && s.top <= to);
}

/** The page shown at a vertical position (the one under it, or the nearest). */
export function pageAt(l: Layout, y: number): number {
  let best = l.slots[0]?.page ?? 1;
  let bestD = Number.POSITIVE_INFINITY;
  for (const s of l.slots) {
    const d = y < s.top ? s.top - y : y > s.top + s.height ? y - (s.top + s.height) : 0;
    if (d < bestD) {
      bestD = d;
      best = s.page;
      if (d === 0) break;
    }
  }
  return best;
}

/** A page-space point as scroll-area pixels. */
export function toScreen(l: Layout, page: number, x: number, y: number): { x: number; y: number } | null {
  const s = l.slots.find((t) => t.page === page);
  return s ? { x: s.left + x * l.scale, y: s.top + y * l.scale } : null;
}

/** A scroll-area point as page space, on the page under it. */
export function toPage(l: Layout, px: number, py: number): { page: number; x: number; y: number } | null {
  const page = pageAt(l, py);
  const s = l.slots.find((t) => t.page === page);
  return s ? { page, x: (px - s.left) / l.scale, y: (py - s.top) / l.scale } : null;
}
