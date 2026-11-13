// Minimal page view (P1.M1.HST.4): PDF.js renders one page of a stored PDF, fetched through the
// host's `ep` protocol with range requests. The full viewer (scroll, zoom, layers) is M2.

import { t } from "@electropaper/kit";
import { type JSX, createEffect, createSignal, on, onCleanup, Show } from "solid-js";
import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { Button } from "@electropaper/kit";
import { blobUrl } from "./host";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

// Everything PDF.js loads comes from the app itself (copied by scripts/copy-pdfjs.mjs).
// `isEvalSupported: false`: the app's CSP forbids eval, so glyphs are drawn without it.
const DOC_OPTIONS = {
  disableAutoFetch: true,
  rangeChunkSize: 262144,
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  wasmUrl: "/pdfjs/wasm/",
  iccUrl: "/pdfjs/iccs/",
  isEvalSupported: false,
} as const;

export function PageView(props: { hash: string }): JSX.Element {
  let canvas!: HTMLCanvasElement;
  let host!: HTMLDivElement;
  const [doc, setDoc] = createSignal<pdfjs.PDFDocumentProxy | null>(null);
  const [page, setPage] = createSignal(1);
  const [error, setError] = createSignal<string | null>(null);
  let task: pdfjs.RenderTask | null = null;

  createEffect(
    on(
      () => props.hash,
      (hash) => {
        setDoc(null);
        setPage(1);
        setError(null);
        const loading = pdfjs.getDocument({ url: blobUrl(hash), ...DOC_OPTIONS });
        loading.promise.then(setDoc, (e: unknown) => setError(String(e)));
        onCleanup(() => {
          void loading.destroy();
        });
      },
    ),
  );

  createEffect(() => {
    const d = doc();
    const n = page();
    if (!d) return;
    void (async () => {
      task?.cancel();
      const p = await d.getPage(n);
      const base = p.getViewport({ scale: 1 });
      const width = Math.max(200, host.clientWidth - 48);
      const viewport = p.getViewport({ scale: width / base.width });
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${Math.floor(viewport.width)}px`;
      canvas.style.height = `${Math.floor(viewport.height)}px`;
      task = p.render({ canvas, viewport, transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0] });
      try {
        await task.promise;
      } catch (e) {
        if (!(e instanceof pdfjs.RenderingCancelledException)) setError(String(e));
      }
    })();
  });

  return (
    <div class="page-view" ref={host}>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
      <Show when={!doc() && !error()}>
        <p class="muted">{t("viewer.loading")}</p>
      </Show>
      <Show when={doc()}>
        {(d) => (
          <div class="page-nav">
            <Button disabled={page() <= 1} onClick={() => setPage(page() - 1)}>
              ‹
            </Button>
            <span>{t("viewer.page", { n: page(), total: d().numPages })}</span>
            <Button disabled={page() >= d().numPages} onClick={() => setPage(page() + 1)}>
              ›
            </Button>
          </div>
        )}
      </Show>
      <canvas class="page" ref={canvas} />
    </div>
  );
}
