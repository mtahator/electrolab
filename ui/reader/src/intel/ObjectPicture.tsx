// A figure, table or equation as it looks on the page (P1.M4.OBJ.6): cut from the page by the engine
// in the paper's colours, loaded when it scrolls into view.

import type { Paper } from "@electropaper/bindings";
import { type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";

import { bus, renderUrl } from "../host";
import { pageColors } from "../viewer/Viewer";

export interface PictureArea {
  page: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function ObjectPicture(props: { file: Paper["file"]; rect: PictureArea; label?: string | null; equation?: boolean }): JSX.Element {
  let box!: HTMLSpanElement;
  const [url, setUrl] = createSignal<string | null>(null);
  onMount(() => {
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        io.disconnect();
        const colors = pageColors();
        const width = Math.round(Math.min(900, 280 * (window.devicePixelRatio || 1)));
        void bus("document.region", { file: props.file, page: props.rect.page, rect: props.rect, exact: true, width_px: width, ...(colors ? { colors } : {}) })
          .then((h) => setUrl(renderUrl(h.image.hash)))
          .catch(() => {});
      },
      { rootMargin: "200px" },
    );
    io.observe(box);
    onCleanup(() => io.disconnect());
  });
  return (
    <span class="jump-picture" classList={{ equation: props.equation ?? false }} ref={box}>
      <Show when={url()} fallback={<span class="jump-picture-wait" />}>
        {(u) => <img src={u()} alt={props.label ?? ""} draggable={false} />}
      </Show>
    </span>
  );
}
