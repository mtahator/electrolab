// One page: the drawing (PDF.js, or a PDFium image for pages PDF.js draws slowly), the text layer
// for selection, the link layer and search highlights. Sizes come from the layout; the backing
// store is redrawn only when the zoom settles, so zoom gestures are CSS-scaled (no blank flash).

import type { BlobRef, Link, PageRect, PageText } from "@electropaper/bindings";
import { t } from "@electropaper/kit";
import { For, type JSX, Show, createEffect, createSignal, onCleanup } from "solid-js";

import { bus, renderUrl } from "../host";
import type { PDFDocumentProxy, RenderTask } from "./pdf";
import { enqueue } from "./queue";

/** Largest backing store for one page (pixels), so high zoom does not exhaust memory. */
const MAX_PIXELS = 16_000_000;
/** Hybrid rule (spike S1): a page PDF.js draws slower than this is drawn by PDFium next time. */
const HEAVY_MS = 150;

export interface PageProps {
  file: BlobRef;
  doc: PDFDocumentProxy;
  page: number;
  width: number;
  height: number;
  /** CSS pixels per point the slot is laid out with (changes live during a zoom gesture). */
  layoutScale: number;
  /** Settled CSS pixels per point (the drawing is redone when it changes). */
  renderScale: number;
  /** Distance from the view centre, for the render queue. */
  priority: () => number;
  heavy: Set<number>;
  markHeavy: (page: number) => void;
  /** Page colours for dark/night/contrast themes (text and lines only), or null. */
  colors: { background: string; foreground: string } | null;
  text: PageText | undefined;
  links: Link[] | undefined;
  highlights: PageRect[];
  activeHighlight: PageRect | null;
  onInternalLink: (page: number, top: number | null) => void;
  onLinkHover: (target: { page: number; top: number | null } | null, at: DOMRect | null) => void;
  onExternalLink: (url: string) => void;
}

function box(r: PageRect, scale: number): JSX.CSSProperties {
  return {
    left: `${r.x0 * scale}px`,
    top: `${r.y0 * scale}px`,
    width: `${(r.x1 - r.x0) * scale}px`,
    height: `${(r.y1 - r.y0) * scale}px`,
  };
}

export function Page(props: PageProps): JSX.Element {
  let host!: HTMLDivElement;
  const [image, setImage] = createSignal<string | null>(null);

  createEffect(() => {
    const scale = props.renderScale;
    const colors = props.colors;
    const pageNo = props.page;
    const heavy = props.heavy.has(pageNo);
    let task: RenderTask | null = null;
    const cancel = enqueue({
      priority: props.priority,
      run: async (signal) => {
        const ratio = window.devicePixelRatio || 1;
        if (heavy && !colors) {
          const r = await bus("document.render", { file: props.file, page: pageNo, width_px: Math.round(props.width * ratio) });
          if (!signal.aborted) setImage(renderUrl(r.image.hash));
          return;
        }
        const page = await props.doc.getPage(pageNo);
        if (signal.aborted) return;
        let s = scale * ratio;
        const base = page.getViewport({ scale: 1 });
        const area = base.width * base.height * s * s;
        if (area > MAX_PIXELS) s *= Math.sqrt(MAX_PIXELS / area);
        const viewport = page.getViewport({ scale: s });
        const canvas = document.createElement("canvas");
        canvas.width = Math.floor(viewport.width);
        canvas.height = Math.floor(viewport.height);
        canvas.className = "page-canvas";
        const started = performance.now();
        task = page.render({ canvas, viewport, ...(colors ? { pageColors: colors } : {}) });
        signal.addEventListener("abort", () => task?.cancel());
        await task.promise;
        if (signal.aborted) return;
        if (performance.now() - started > HEAVY_MS) props.markHeavy(pageNo);
        // Swap only when the new drawing is complete: the old one stays visible meanwhile.
        const old = host.querySelector("canvas.page-canvas");
        if (old) old.replaceWith(canvas);
        else host.prepend(canvas);
        setImage(null);
      },
    });
    onCleanup(() => {
      cancel();
      task?.cancel();
    });
  });

  return (
    <div class="page" ref={host} style={{ width: `${props.width}px`, height: `${props.height}px` }} data-page={props.page}>
      <Show when={image()}>{(src) => <img class="page-canvas" src={src()} alt="" draggable={false} />}</Show>
      <div class="highlight-layer">
        <For each={props.highlights}>{(r) => <div class="hit" classList={{ active: props.activeHighlight === r }} style={box(r, props.layoutScale)} />}</For>
      </div>
      <TextLayer text={props.text} scale={props.layoutScale} />
      <div class="link-layer">
        <For each={props.links ?? []}>
          {(l) => {
            const target = l.target;
            if (target.kind === "page") {
              let timer: number | undefined;
              return (
                <button
                  type="button"
                  class="link"
                  style={box(l.rect, props.layoutScale)}
                  aria-label={t("viewer.page_box") + ` ${target.page}`}
                  onClick={() => props.onInternalLink(target.page, target.top ?? null)}
                  onMouseEnter={(e) => {
                    const at = e.currentTarget.getBoundingClientRect();
                    timer = window.setTimeout(() => props.onLinkHover({ page: target.page, top: target.top ?? null }, at), 400);
                  }}
                  onMouseLeave={() => {
                    window.clearTimeout(timer);
                    props.onLinkHover(null, null);
                  }}
                />
              );
            }
            if (target.kind === "url") {
              return (
                <button
                  type="button"
                  class="link"
                  style={box(l.rect, props.layoutScale)}
                  title={t("viewer.link_external", { url: target.url })}
                  onClick={() => props.onExternalLink(target.url)}
                />
              );
            }
            return <span class="link unsupported" style={box(l.rect, props.layoutScale)} title={t("viewer.not_pdf_link")} />;
          }}
        </For>
      </div>
    </div>
  );
}

/** Transparent words over the drawing, for selecting and copying (P1.M2.VW.5). Each word carries
 *  its character range, so copied text comes from the Document engine, not from the DOM. */
function TextLayer(props: { text: PageText | undefined; scale: number }): JSX.Element {
  return (
    <div class="text-layer">
      <Show when={props.text}>
        {(pt) => {
          const chars = Array.from(pt().text);
          return (
            <For each={pt().words}>
              {(w) => {
                const h = (w.rect.y1 - w.rect.y0) * props.scale;
                return (
                  <span
                    data-page={pt().page}
                    data-start={w.start}
                    data-end={w.end}
                    style={{
                      left: `${w.rect.x0 * props.scale}px`,
                      top: `${w.rect.y0 * props.scale}px`,
                      width: `${(w.rect.x1 - w.rect.x0) * props.scale}px`,
                      height: `${h}px`,
                      "font-size": `${h * 0.85}px`,
                    }}
                  >
                    {chars.slice(w.start, w.end).join("").replace("­", "")}
                  </span>
                );
              }}
            </For>
          );
        }}
      </Show>
    </div>
  );
}

/** Text of the current selection, taken from the engine's characters in reading order
 *  (soft-hyphenated line breaks joined, other line breaks become spaces). */
export function selectedText(texts: (page: number) => PageText | undefined): string | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  const spans = Array.from(document.querySelectorAll<HTMLSpanElement>(".text-layer span[data-page]")).filter((s) => range.intersectsNode(s));
  if (spans.length === 0) return null;
  const byPage = new Map<number, { start: number; end: number }>();
  for (const s of spans) {
    const page = Number(s.dataset.page);
    const start = Number(s.dataset.start);
    const end = Number(s.dataset.end);
    const cur = byPage.get(page);
    byPage.set(page, cur ? { start: Math.min(cur.start, start), end: Math.max(cur.end, end) } : { start, end });
  }
  const parts: string[] = [];
  for (const [page, r] of [...byPage.entries()].sort((a, b) => a[0] - b[0])) {
    const pt = texts(page);
    if (!pt) continue;
    parts.push(Array.from(pt.text).slice(r.start, r.end).join(""));
  }
  return parts
    .join("\n")
    .replace(/­\n/g, "")
    .replace(/[ \t]*\n[ \t]*/g, " ")
    .replace(/­/g, "")
    .trim();
}
