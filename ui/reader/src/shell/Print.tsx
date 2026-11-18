// Printing prints the paper, never the window, and needs no other app (design language §17.9).
// The sheet asks which pages and whether to include the annotations; the chosen pages are
// drawn sharp at their own size, placed in a print-only layer, and the standard print dialog
// then shows exactly those pages.

import type { BlobRef, Paper } from "@electropaper/bindings";
import { Button, Dialog, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup } from "solid-js";
import { Portal } from "solid-js/web";

import { annotationsOf } from "../annotations/state";
import { bus, renderUrl } from "../host";

/** "1-5, 8, 11-13" → page numbers within 1..n, in order; null when it cannot be read. */
export function parseRange(text: string, n: number): number[] | null {
  const out = new Set<number>();
  for (const part of text.split(",").map((x) => x.trim()).filter(Boolean)) {
    const m = part.match(/^(\d+)(?:\s*[-–]\s*(\d+))?$/);
    if (!m) return null;
    const a = Number(m[1]);
    const b = m[2] ? Number(m[2]) : a;
    if (a < 1 || b < a || a > n) return null;
    for (let p = a; p <= Math.min(b, n); p++) out.add(p);
  }
  return out.size > 0 ? [...out].sort((x, y) => x - y) : null;
}

/** Pixels across a printed page: about 200 dpi on letter or A4; less for very long prints so
 *  memory stays reasonable. */
const widthFor = (pages: number): number => (pages <= 100 ? 1700 : 1200);

export function PrintSheet(props: { paper: Paper; pages: number; current: number; onClose: () => void }): JSX.Element {
  const hasMarks = (): boolean => (annotationsOf(props.paper.id)?.annotations.length ?? 0) > 0;
  const [which, setWhich] = createSignal<"all" | "current" | "range">("all");
  const [range, setRange] = createSignal("");
  const [withMarks, setWithMarks] = createSignal(hasMarks());
  const [progress, setProgress] = createSignal<{ done: number; total: number } | null>(null);
  const [images, setImages] = createSignal<string[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  let cancelled = false;
  onCleanup(() => {
    cancelled = true;
  });

  const chosen = (): number[] | null => {
    if (which() === "all") return Array.from({ length: props.pages }, (_, i) => i + 1);
    if (which() === "current") return [props.current];
    return parseRange(range(), props.pages);
  };

  const close = (): void => {
    cancelled = true;
    setImages([]);
    props.onClose();
  };

  const print = async (): Promise<void> => {
    const pages = chosen();
    if (!pages) {
      setError(t("print.bad_range", { n: props.pages }));
      return;
    }
    cancelled = false;
    setError(null);
    setProgress({ done: 0, total: pages.length });
    try {
      // With annotations: an annotated copy of the file is drawn; the paper is not changed.
      const marks = withMarks() && hasMarks();
      const file: BlobRef = marks ? (await bus("document.annotate", { paper: props.paper.id, file: props.paper.file })).file : props.paper.file;
      const width = widthFor(pages.length);
      const urls: string[] = [];
      for (const p of pages) {
        if (cancelled) return;
        const r = await bus("document.render", { file, page: p, width_px: width, annotations: marks });
        const url = renderUrl(r.image.hash);
        const img = new Image();
        img.src = url;
        await img.decode().catch(() => {});
        urls.push(url);
        setProgress({ done: urls.length, total: pages.length });
      }
      if (cancelled) return;
      setImages(urls);
      // The print layer is in the page now: open the standard print dialog, then close.
      requestAnimationFrame(() => {
        const done = (): void => {
          window.removeEventListener("afterprint", done);
          close();
        };
        window.addEventListener("afterprint", done);
        window.print();
      });
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
      setProgress(null);
    }
  };

  return (
    <>
      <Dialog
        title={t("print.title")}
        onCancel={close}
        actions={
          <>
            <Button onClick={close}>{t("common.cancel")}</Button>
            <Button variant="primary" icon="printer" disabled={progress() !== null} onClick={() => void print()}>
              {t("print.print")}
            </Button>
          </>
        }
      >
        <div class="print-sheet">
          <label class="check">
            <input type="radio" name="print-pages" checked={which() === "all"} onChange={() => setWhich("all")} />
            {t("print.all", { n: props.pages })}
          </label>
          <label class="check">
            <input type="radio" name="print-pages" checked={which() === "current"} onChange={() => setWhich("current")} />
            {t("print.current", { n: props.current })}
          </label>
          <label class="check print-range">
            <input type="radio" name="print-pages" checked={which() === "range"} onChange={() => setWhich("range")} />
            {t("print.range")}
            <input
              class="ep-input"
              placeholder="1-5, 8, 11-13"
              value={range()}
              onFocus={() => setWhich("range")}
              onInput={(e) => setRange(e.currentTarget.value)}
              aria-label={t("print.range")}
            />
          </label>
          <label class="check">
            <input type="checkbox" checked={withMarks()} disabled={!hasMarks()} onChange={(e) => setWithMarks(e.currentTarget.checked)} />
            {t("print.annotations")}
          </label>
          <Show when={progress()}>
            {(p) => (
              <p class="muted print-progress">
                <span class="spinner" aria-hidden="true" /> {t("print.preparing", { done: p().done, total: p().total })}
              </p>
            )}
          </Show>
          <Show when={error()}>
            <p class="error">{error()}</p>
          </Show>
        </div>
      </Dialog>
      <Show when={images().length > 0}>
        <Portal>
          <div class="print-root" aria-hidden="true">
            <For each={images()}>{(src) => <img class="print-page" src={src} alt="" />}</For>
          </div>
        </Portal>
      </Show>
    </>
  );
}
