// Page previews and the drawn-page cache, so a page is never blank (P1.M4 polish).
//
// - Previews: a small drawing of every page, made in the background right after a paper opens
//   (pages near the view first). A page shows its preview at once, and the sharp drawing fades in
//   over it when ready. Previews are cheap (about 10 ms each with PDFium) and shared with the
//   thumbnails panel.
// - Drawn pages: the last sharp drawing of recently shown pages, so scrolling back to a page shows
//   it at once instead of asking for it again.

import type { BlobRef } from "@electropaper/bindings";
import { createStore } from "solid-js/store";

import { bus, renderUrl } from "../host";
import { enqueue } from "./queue";

/** Width of previews and thumbnails, in device pixels. */
export const previewWidth = (): number => Math.round(160 * (window.devicePixelRatio || 1));

const [previews, setPreviews] = createStore<Record<string, Record<number, string>>>({});
const loading = new Map<string, { users: number; cancel: () => void; done: Promise<void>; ensure: (page: number) => void }>();

type Colors = { background: string; foreground: string } | null;

/** Previews are kept per file and paper colours (dark papers have their own). */
export function previewKey(hash: string, colors: Colors): string {
  return colors ? `${hash}|${colors.background}|${colors.foreground}` : hash;
}

/** Settles when every preview of a file is made (background work that shares the PDF engine
 *  waits for it, so pages are drawn first). */
export function previewsDone(key: string): Promise<void> {
  return loading.get(key)?.done ?? Promise.resolve();
}

/** The preview image of a page, once made. */
export function previewOf(key: string, page: number): string | undefined {
  return previews[key]?.[page];
}

/** Pages on each side of the view that get previews (design language §17.2: the engine is
 *  shared, so previews are made only near where the reader is, as background work). */
const AHEAD = 40;

/** Make previews of the pages near `near()`, nearest first, and more as the reader moves
 *  (`previewsNear`). Returns a function that stops (when the last viewer of the file closes). */
export function loadPreviews(file: BlobRef, pages: number, near: () => number, colors: Colors = null): () => void {
  const hash = previewKey(file.hash, colors);
  const cur = loading.get(hash);
  if (cur) {
    cur.users += 1;
  } else {
    if (!previews[hash]) setPreviews(hash, {});
    const cancels = new Map<number, () => void>();
    let left = 0;
    let finished = (): void => {};
    const done = new Promise<void>((ok) => (finished = ok));
    const ensure = (center: number): void => {
      const from = Math.max(1, center - AHEAD);
      const to = Math.min(pages, center + AHEAD);
      for (let p = from; p <= to; p++) {
        if (previews[hash]?.[p] || cancels.has(p)) continue;
        left += 1;
        cancels.set(
          p,
          enqueue({
            // Ahead of sharp drawings of pages out of view (whose priority is their distance in
            // pixels), behind those in view (0): a preview costs a few milliseconds.
            priority: () => 1 + Math.abs(p - near()) * 40,
            background: true,
            run: async (signal) => {
              const r = await bus("document.render", { file, page: p, width_px: previewWidth(), background: true, ...(colors ? { colors } : {}) }).finally(() => {
                left -= 1;
                if (left <= 0) finished();
              });
              if (signal.aborted) return;
              const url = renderUrl(r.image.hash);
              const img = new Image();
              img.src = url;
              await Promise.race([img.decode().catch(() => {}), new Promise((ok) => setTimeout(ok, 3000))]);
              setPreviews(hash, p, url);
            },
          }),
        );
      }
      if (left === 0) finished();
    };
    ensure(near());
    loading.set(hash, { users: 1, cancel: () => cancels.forEach((c) => c()), done, ensure });
  }
  return () => {
    const e = loading.get(hash);
    if (!e) return;
    e.users -= 1;
    if (e.users <= 0) {
      e.cancel();
      loading.delete(hash);
    }
  };
}

/** The reader moved: make previews around `page` too. */
export function previewsNear(key: string, page: number): void {
  loading.get(key)?.ensure(page);
}

// ---- drawn pages ---------------------------------------------------------------------------------

const DRAWN_KEEP = 48;
const drawn = new Map<string, { url: string; width: number }>();

/** The last sharp drawing of a page (any width), if it was shown recently. */
export function drawnOf(hash: string, page: number): { url: string; width: number } | undefined {
  const k = `${hash}:${page}`;
  const d = drawn.get(k);
  if (d) {
    // Most recently used last.
    drawn.delete(k);
    drawn.set(k, d);
  }
  return d;
}

export function keepDrawn(hash: string, page: number, url: string, width: number): void {
  const k = `${hash}:${page}`;
  drawn.delete(k);
  drawn.set(k, { url, width });
  while (drawn.size > DRAWN_KEEP) {
    const first = drawn.keys().next().value;
    if (first === undefined) break;
    drawn.delete(first);
  }
}
