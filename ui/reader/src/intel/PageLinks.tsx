// Mentions and citation markers on a page (P1.M4.LNK.3, guideline §1 "quiet until asked"): a
// faint dotted underline; hover (after a short pause) or click shows the figure, table, equation
// or reference in a popover. Ctrl+drag (or Ctrl+click) picks a region of the page (P1.M4.OBJ.5).

import type { BlobRef, CitationMarker, Mention, PageQuad } from "@electropaper/bindings";
import { t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";

import { bus } from "../host";
import { linksOf, setPeek } from "./state";

const HOVER_MS = 450;

function quadBox(q: PageQuad, scale: number): JSX.CSSProperties {
  const xs = q.p.map((p) => p.x);
  const ys = q.p.map((p) => p.y);
  return {
    left: `${Math.min(...xs) * scale}px`,
    top: `${Math.min(...ys) * scale}px`,
    width: `${(Math.max(...xs) - Math.min(...xs)) * scale}px`,
    height: `${(Math.max(...ys) - Math.min(...ys)) * scale}px`,
  };
}

export function PageLinks(props: { file: BlobRef; page: number; scale: number }): JSX.Element {
  const [mentions, setMentions] = createSignal<Mention[]>([]);
  const [citations, setCitations] = createSignal<CitationMarker[]>([]);
  let timer: number | undefined;
  onMount(() => {
    void linksOf(props.file, props.page).then((l) => {
      setMentions(l.mentions.filter((m) => m.target));
      setCitations(l.citations);
    });
  });
  onCleanup(() => window.clearTimeout(timer));

  const openMention = async (m: Mention, at: DOMRect): Promise<void> => {
    const target = m.target;
    if (!target) return;
    const objs = await bus("document.objects", { file: props.file, page: target.page }).catch(() => null);
    const object = objs?.objects.find((o) => o.id === target.id);
    if (object) setPeek({ kind: "object", at, file: props.file, object, mention: m });
  };
  const hover = (f: () => void): void => {
    window.clearTimeout(timer);
    timer = window.setTimeout(f, HOVER_MS);
  };

  return (
    <div class="link-marks" aria-hidden="false">
      <For each={mentions()}>
        {(m) => (
          <For each={m.quads}>
            {(q) => (
              <button
                type="button"
                class="mention"
                style={quadBox(q, props.scale)}
                aria-label={`${m.text}: ${t("intel.show")}`}
                onClick={(e) => void openMention(m, e.currentTarget.getBoundingClientRect())}
                onMouseEnter={(e) => {
                  const at = e.currentTarget.getBoundingClientRect();
                  hover(() => void openMention(m, at));
                }}
                onMouseLeave={() => window.clearTimeout(timer)}
              />
            )}
          </For>
        )}
      </For>
      <For each={citations()}>
        {(c) => (
          <For each={c.quads}>
            {(q) => (
              <button
                type="button"
                class="mention citation"
                style={quadBox(q, props.scale)}
                aria-label={`${c.text}: ${t("intel.show_reference")}`}
                onClick={(e) => setPeek({ kind: "citation", at: e.currentTarget.getBoundingClientRect(), file: props.file, marker: c })}
                onMouseEnter={(e) => {
                  const at = e.currentTarget.getBoundingClientRect();
                  hover(() => setPeek({ kind: "citation", at, file: props.file, marker: c }));
                }}
                onMouseLeave={() => window.clearTimeout(timer)}
              />
            )}
          </For>
        )}
      </For>
    </div>
  );
}

/** Ctrl+drag or Ctrl+click on the page: pick a region (snaps to the object under it). */
export function RegionTool(props: { file: BlobRef; page: number; scale: number }): JSX.Element {
  let host!: HTMLDivElement;
  const [drag, setDrag] = createSignal<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  onMount(() => {
    const page = host.parentElement;
    if (!page) return;
    const down = (e: PointerEvent): void => {
      if (!e.ctrlKey || e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      const r = page.getBoundingClientRect();
      const pt = (ev: PointerEvent): { x: number; y: number } => ({ x: (ev.clientX - r.left) / props.scale, y: (ev.clientY - r.top) / props.scale });
      const s = pt(e);
      setDrag({ x0: s.x, y0: s.y, x1: s.x, y1: s.y });
      const move = (ev: PointerEvent): void => {
        const p = pt(ev);
        setDrag({ x0: s.x, y0: s.y, x1: p.x, y1: p.y });
      };
      const up = (ev: PointerEvent): void => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        const d = drag();
        setDrag(null);
        if (!d) return;
        const rect = { x0: Math.min(d.x0, d.x1), y0: Math.min(d.y0, d.y1), x1: Math.max(d.x0, d.x1), y1: Math.max(d.y0, d.y1) };
        const at = new DOMRect(r.left + rect.x0 * props.scale, r.top + rect.y0 * props.scale, (rect.x1 - rect.x0) * props.scale, (rect.y1 - rect.y0) * props.scale);
        // A click (tiny drag) picks the object under the point.
        const click = (rect.x1 - rect.x0) * props.scale < 4 && (rect.y1 - rect.y0) * props.scale < 4;
        setPeek({ kind: "region", at: click ? new DOMRect(ev.clientX, ev.clientY, 1, 1) : at, file: props.file, page: props.page, rect: click ? { x0: s.x, y0: s.y, x1: s.x, y1: s.y } : rect });
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    };
    page.addEventListener("pointerdown", down, true);
    onCleanup(() => page.removeEventListener("pointerdown", down, true));
  });
  return (
    <div class="region-host" ref={host}>
      <Show when={drag()}>
        {(d) => (
          <div
            class="region-drag"
            style={{
              left: `${Math.min(d().x0, d().x1) * props.scale}px`,
              top: `${Math.min(d().y0, d().y1) * props.scale}px`,
              width: `${Math.abs(d().x1 - d().x0) * props.scale}px`,
              height: `${Math.abs(d().y1 - d().y0) * props.scale}px`,
            }}
          />
        )}
      </Show>
    </div>
  );
}
