// The viewer (P1.M2.VW): virtualised pages in continuous, single or two-page layout; zoom with the
// point under the cursor kept still; keyboard; back/forward history; link previews; text copied
// from the Document engine; reading position and reading time saved per paper; annotations
// (P1.M3): the selection bar, text-mark tools and the annotation keys.

import type { DocInfo, Link, PageRect, PageText, Paper, ReadingState } from "@electropaper/bindings";
import { paper, t } from "@electropaper/kit";
import { For, type JSX, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";

import { citation } from "../annotations/geometry";
import { MARGIN_W, MarginNotes } from "../annotations/MarginNotes";
import { SelectionBar } from "../annotations/SelectionBar";
import { marksFor, selectionBox, selectionRanges } from "../annotations/selection";
import { transformed } from "../annotations/AnnotationLayer";
import { add, change, find, isMarkup, load as loadAnnotations, pending, picked, remove, setPending, setPicked, setTool, tool, typing } from "../annotations/state";
import { bus, openExternal } from "../host";
import { quoteToNotes } from "../notes/state";
import { marginView } from "../shell/frame";
import { History, type Place } from "./history";
import { type Layout, type ViewMode, type Zoom, clamp, layout, pageAt, scaleFor, toPage, toScreen, visible } from "./layout";
import { Page, selectedText } from "./Page";
import { type PDFDocumentProxy, acquire, release } from "./pdf";

/** What the shell can do with a viewer. */
export interface ViewerApi {
  goTo: (page: number, y?: number | null, record?: boolean) => void;
  back: () => void;
  forward: () => void;
  zoomBy: (factor: number) => void;
  setZoom: (z: Zoom) => void;
  setMode: (m: ViewMode) => void;
  focus: () => void;
  /** Copy the selected text with its citation and page; false when nothing is selected. */
  copyCitation: () => boolean;
  /** Briefly mark the line at page-space `y` (after going to a place from the notes). */
  flash: (page: number, y: number) => void;
}

/** What the viewer tells the shell. */
export interface ViewerStatus {
  page: number;
  pages: number;
  scale: number;
  zoom: Zoom;
  mode: ViewMode;
  canBack: boolean;
  canForward: boolean;
}

/** Search hits to mark: per page, the boxes of each hit. */
export interface Marks {
  hits: { page: number; rects: PageRect[] }[];
  active: number | null;
}

export interface ViewerProps {
  paper: Paper;
  active: boolean;
  marks: Marks;
  onReady: (api: ViewerApi) => void;
  onStatus: (s: ViewerStatus) => void;
}

const RECORD_EVERY_S = 15;
/** Pages around the view whose text and links stay loaded. */
const KEEP_NEAR = 10;

function pageColors(): { background: string; foreground: string } | null {
  const css = getComputedStyle(document.documentElement);
  const ink = css.getPropertyValue("--page-ink").trim();
  const bg = css.getPropertyValue("--page-bg").trim();
  return ink ? { background: bg || "#000", foreground: ink } : null;
}


export function Viewer(props: ViewerProps): JSX.Element {
  let scroller!: HTMLDivElement;
  const hash = props.paper.file.hash;
  const [doc, setDoc] = createSignal<PDFDocumentProxy | null>(null);
  const [info, setInfo] = createSignal<DocInfo | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [zoom, setZoomSignal] = createSignal<Zoom>({ kind: "fit_width" });
  const [mode, setModeSignal] = createSignal<ViewMode>("continuous");
  const [view, setView] = createSignal({ w: 800, h: 600, top: 0 });
  const [current, setCurrent] = createSignal(1);
  const [renderScale, setRenderScale] = createSignal(1);
  const [texts, setTexts] = createStore<Record<number, PageText>>({});
  const [links, setLinks] = createStore<Record<number, Link[]>>({});
  const [preview, setPreview] = createSignal<{ page: number; top: number | null; at: DOMRect } | null>(null);
  const [selBar, setSelBar] = createSignal<DOMRect | null>(null);
  const [flashAt, setFlashAt] = createSignal<{ page: number; y: number; n: number } | null>(null);
  const margin = (): number => (marginView(props.paper.id) ? MARGIN_W : 0);
  const history = new History();
  const [histTick, setHistTick] = createSignal(0);
  let restored = false;
  let saveTimer: number | undefined;

  const sizes = (): DocInfo["sizes"] => info()?.sizes ?? [];
  const scale = createMemo(() => scaleFor(zoom(), sizes(), mode(), view().w - margin(), view().h));
  // Only the single-page layout depends on the current page; in the other layouts a page change
  // must not rebuild (and redraw) every page.
  const layoutPage = createMemo(() => (mode() === "single" ? current() : 1));
  const lay = createMemo<Layout>(() => layout(sizes(), scale(), mode(), view().w - margin(), layoutPage()));
  const shown = createMemo(() => visible(lay(), view().top - view().h, view().top + 2 * view().h));
  // Page colours follow the paper setting (independent of the app theme).
  const colors = createMemo(() => {
    paper();
    return pageColors();
  });

  // Redraw sharply only when the zoom has settled (CSS scaling meanwhile).
  createEffect(
    on(scale, (s) => {
      const id = window.setTimeout(() => setRenderScale(s), 180);
      onCleanup(() => window.clearTimeout(id));
    }),
  );

  const here = (): Place => {
    const l = lay();
    const page = mode() === "single" ? current() : pageAt(l, view().top + 1);
    const s = l.slots.find((x) => x.page === page);
    return { page, y: s ? Math.max(0, (view().top - s.top) / l.scale) : 0 };
  };

  /** Show `page` with page-space `y` near the top. `exact` puts `y` exactly at the top (used to
   *  restore a saved position, which `here()` measured the same way). */
  const goTo = (page: number, y: number | null = null, record = true, exact = false): void => {
    const n = info()?.pages ?? 1;
    const p = Math.min(Math.max(1, Math.round(page)), n);
    if (record) {
      history.push(here());
      setHistTick((v) => v + 1);
      // A deliberate jump is a place worth keeping: save soon, not only when scrolling stops.
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => void save(), 400);
    }
    if (mode() === "single") setCurrent(p);
    queueMicrotask(() => {
      const pt = toScreen(lay(), p, 0, y ?? 0);
      if (pt) scroller.scrollTop = Math.max(0, pt.y - (exact ? 0 : y === null ? 16 : 40));
    });
  };

  const zoomAround = (z: Zoom, clientX?: number, clientY?: number): void => {
    const rect = scroller.getBoundingClientRect();
    const cx = clientX === undefined ? rect.width / 2 : clientX - rect.left;
    const cy = clientY === undefined ? rect.height / 2 : clientY - rect.top;
    const anchor = toPage(lay(), scroller.scrollLeft + cx, scroller.scrollTop + cy);
    setZoomSignal(z);
    if (!anchor) return;
    queueMicrotask(() => {
      const pt = toScreen(lay(), anchor.page, anchor.x, anchor.y);
      if (pt) {
        scroller.scrollLeft = pt.x - cx;
        scroller.scrollTop = pt.y - cy;
      }
    });
  };

  const api: ViewerApi = {
    goTo,
    back: () => {
      const p = history.goBack(here());
      if (p) goTo(p.page, p.y, false, true);
      setHistTick((v) => v + 1);
    },
    forward: () => {
      const p = history.goForward(here());
      if (p) goTo(p.page, p.y, false, true);
      setHistTick((v) => v + 1);
    },
    zoomBy: (f) => zoomAround({ kind: "scale", factor: clamp(scale() * f) }),
    setZoom: (z) => zoomAround(z),
    setMode: (m) => {
      const p = here();
      setModeSignal(m);
      setCurrent(p.page);
      goTo(p.page, p.y, false, true);
    },
    focus: () => scroller.focus(),
    copyCitation: () => {
      const text = selectedText((p) => texts[p]);
      const ranges = selectionRanges((p) => texts[p]);
      if (!text || ranges.length === 0) return false;
      void navigator.clipboard.writeText(citation(text, props.paper, ranges[0]?.page ?? current())).catch(() => {});
      return true;
    },
    flash: (page, y) => {
      const n = (flashAt()?.n ?? 0) + 1;
      setFlashAt({ page, y, n });
      window.setTimeout(() => setFlashAt((f) => (f?.n === n ? null : f)), 1400);
    },
  };

  // ---- annotations: the selection bar and the text-mark tools (P1.M3.UI.2, UI.9) -------------
  const markSelection = (kind: "highlight" | "underline" | "strike_out" | "squiggly", color?: string): void => {
    const marks = marksFor(kind, selectionRanges((p) => texts[p]), (p) => texts[p], color);
    window.getSelection()?.removeAllRanges();
    setSelBar(null);
    if (marks.length > 0) void add(props.paper.id, marks);
  };
  const commentSelection = (): void => {
    const marks = marksFor("highlight", selectionRanges((p) => texts[p]), (p) => texts[p]);
    window.getSelection()?.removeAllRanges();
    setSelBar(null);
    const first = marks[0];
    if (!first) return;
    // The comment belongs to the first page's mark; marks on further pages are added with it.
    if (marks.length > 1) void add(props.paper.id, marks.slice(1));
    setPending({ paper: props.paper.id, annotation: first });
  };
  const onMouseUp = (): void => {
    // Let the browser finish the selection first.
    window.setTimeout(() => {
      const box = selectionBox();
      if (!box || selectionRanges((p) => texts[p]).length === 0) return setSelBar(null);
      const k = tool();
      if (isMarkup(k)) markSelection(k);
      else if (k === "select") setSelBar(box);
    }, 0);
  };
  onMount(() => {
    const onSel = (): void => {
      const s = window.getSelection();
      if (!s || s.isCollapsed) setSelBar(null);
    };
    document.addEventListener("selectionchange", onSel);
    onCleanup(() => document.removeEventListener("selectionchange", onSel));
  });

  onMount(async () => {
    props.onReady(api);
    // Measure now, so a restored position is laid out at the real size (not a placeholder).
    setView((v) => ({ ...v, w: scroller.clientWidth || v.w, h: scroller.clientHeight || v.h }));
    const ro = new ResizeObserver(() => setView((v) => ({ ...v, w: scroller.clientWidth, h: scroller.clientHeight })));
    ro.observe(scroller);
    onCleanup(() => ro.disconnect());
    void loadAnnotations(props.paper);
    try {
      const [i, d, st] = await Promise.all([bus("document.info", { file: props.paper.file }), acquire(hash), bus("workspace.get_reading_state", { paper: props.paper.id })]);
      setInfo(i);
      setDoc(d);
      restore(st);
    } catch (e) {
      setError(String((e as Error).message ?? e));
    }
  });
  // Save when the window is hidden or closed too (closing does not always run cleanups).
  const onHide = (): void => {
    if (document.visibilityState === "hidden") void save();
  };
  document.addEventListener("visibilitychange", onHide);
  window.addEventListener("pagehide", onHide);
  onCleanup(() => {
    document.removeEventListener("visibilitychange", onHide);
    window.removeEventListener("pagehide", onHide);
    void save();
    release(hash);
  });

  const restore = (st: ReadingState): void => {
    if (st.view_mode) setModeSignal(st.view_mode);
    const z = st.zoom;
    if (z) setZoomSignal(z.kind === "scale" ? { kind: "scale", factor: z.factor } : { kind: z.kind });
    const loc = st.location;
    if (loc) {
      setCurrent(loc.page);
      requestAnimationFrame(() => goTo(loc.page, loc.target?.y0 ?? 0, false, true));
    }
    restored = true;
  };

  const save = async (): Promise<void> => {
    if (!restored) return;
    const p = here();
    const z = zoom();
    await bus("workspace.save_reading_state", {
      paper: props.paper.id,
      location: { paper: props.paper.id, page: p.page, target: { page: p.page, x0: 0, y0: p.y, x1: 0, y1: p.y } },
      zoom: z.kind === "scale" ? { kind: "scale", factor: z.factor } : { kind: z.kind },
      view_mode: mode(),
    }).catch(() => {});
  };

  // Save the place a moment after scrolling stops.
  const onScroll = (): void => {
    setView((v) => ({ ...v, top: scroller.scrollTop }));
    if (selBar()) setSelBar(selectionBox());
    if (mode() !== "single") setCurrent(pageAt(lay(), scroller.scrollTop + scroller.clientHeight / 2));
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void save(), 1500);
  };

  // Tell the shell what changed.
  createEffect(() => {
    histTick();
    props.onStatus({ page: current(), pages: info()?.pages ?? 0, scale: scale(), zoom: zoom(), mode: mode(), canBack: history.canBack(), canForward: history.canForward() });
  });

  // Text and links of visible pages, fetched once each, when the page is shown and idle.
  createEffect(() => {
    const d = info();
    if (!d) return;
    for (const s of shown()) {
      const p = s.page;
      if (!(p in texts)) {
        setTexts(p, { page: p, text: "", boxes: [], words: [], lines: [], unknown: 0 });
        const idle = (window as Window & { requestIdleCallback?: (f: () => void) => number }).requestIdleCallback ?? ((f: () => void) => window.setTimeout(f, 30));
        idle(() => {
          void bus("document.text", { file: props.paper.file, page: p }).then((tx) => setTexts(p, tx)).catch(() => {});
        });
      }
      if (!(p in links)) {
        setLinks(p, []);
        void bus("document.links", { file: props.paper.file, page: p }).then((ls) => setLinks(p, ls)).catch(() => {});
      }
    }
    // Keep text and links only near the view, so memory stays flat in long papers.
    const near = shown().map((s) => s.page);
    if (near.length > 0) {
      const lo = Math.min(...near) - KEEP_NEAR;
      const hi = Math.max(...near) + KEEP_NEAR;
      const far = (k: string): boolean => Number(k) < lo || Number(k) > hi;
      const farTexts = Object.keys(untrack(() => texts)).filter(far);
      const farLinks = Object.keys(untrack(() => links)).filter(far);
      if (farTexts.length > 0) setTexts(produce((t) => farTexts.forEach((k) => delete t[Number(k)])));
      if (farLinks.length > 0) setLinks(produce((l) => farLinks.forEach((k) => delete l[Number(k)])));
    }
  });

  // Reading time: counted only while this tab is active, the window has focus and is visible.
  onMount(() => {
    const id = window.setInterval(() => {
      if (!props.active || document.hidden || !document.hasFocus() || !info()) return;
      void bus("workspace.record_reading", { paper: props.paper.id, page: current(), seconds: RECORD_EVERY_S }).catch(() => {});
    }, RECORD_EVERY_S * 1000);
    onCleanup(() => window.clearInterval(id));
  });

  const onWheel = (e: WheelEvent): void => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    zoomAround({ kind: "scale", factor: clamp(scale() * Math.exp(-e.deltaY * 0.0015)) }, e.clientX, e.clientY);
  };

  const onKey = (e: KeyboardEvent): void => {
    const h = scroller.clientHeight;
    const single = mode() === "single";
    const key = e.key;
    if (e.altKey && key === "ArrowLeft") return void (e.preventDefault(), api.back());
    if (e.altKey && key === "ArrowRight") return void (e.preventDefault(), api.forward());
    if (e.ctrlKey && (key === "=" || key === "+")) return void (e.preventDefault(), api.zoomBy(1.2));
    if (e.ctrlKey && key === "-") return void (e.preventDefault(), api.zoomBy(1 / 1.2));
    if (e.ctrlKey && key === "0") return void (e.preventDefault(), api.setZoom({ kind: "fit_width" }));
    if (!typing()) {
      if (key === "Escape") {
        if (pending()) setPending(null);
        else if (picked()) setPicked(null);
        else if (tool() !== "select") setTool("select");
        setSelBar(null);
        return;
      }
      const p = picked();
      if ((key === "Delete" || key === "Backspace") && p && p.paper === props.paper.id) {
        e.preventDefault();
        void remove(p.paper, p.id);
        return;
      }
      // Arrow keys move the picked drawing by 1 pt (Shift: 10 pt) — the keyboard way to drag.
      const step = e.shiftKey ? 10 : 1;
      const delta = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[key];
      const a = p && p.paper === props.paper.id ? find(p.paper, p.id) : undefined;
      if (delta && a && !isMarkup(a.kind)) {
        e.preventDefault();
        const [dx, dy] = delta as [number, number];
        void change(p!.paper, transformed(a, { kind: "move", id: a.id, start: { x: 0, y: 0 }, dx, dy, moved: true }));
        return;
      }
    }
    const step = (dir: number): void => {
      if (single) goTo(current() + dir, 0, false);
      else scroller.scrollBy({ top: dir * (h - 40) });
    };
    switch (key) {
      case "PageDown":
        e.preventDefault();
        step(1);
        break;
      case "PageUp":
        e.preventDefault();
        step(-1);
        break;
      case " ":
        e.preventDefault();
        step(e.shiftKey ? -1 : 1);
        break;
      case "Home":
        e.preventDefault();
        goTo(1, 0);
        break;
      case "End":
        e.preventDefault();
        goTo(info()?.pages ?? 1, 0);
        break;
      case "ArrowDown":
        if (single && scroller.scrollTop + h >= scroller.scrollHeight - 2) step(1);
        break;
      case "ArrowUp":
        if (single && scroller.scrollTop <= 0) step(-1);
        break;
      default:
    }
  };

  const onCopy = (e: ClipboardEvent): void => {
    const text = selectedText((p) => texts[p]);
    if (text && e.clipboardData) {
      e.preventDefault();
      e.clipboardData.setData("text/plain", text);
    }
  };

  const hitsOn = (page: number): PageRect[] => props.marks.hits.filter((h) => h.page === page).flatMap((h) => h.rects);
  const activeRect = (page: number): PageRect | null => {
    const a = props.marks.active;
    const hit = a === null ? undefined : props.marks.hits[a];
    return hit && hit.page === page ? (hit.rects[0] ?? null) : null;
  };

  // Bring the active search hit into view.
  createEffect(
    on(
      () => props.marks.active,
      (a) => {
        const hit = a === null ? undefined : props.marks.hits[a];
        const r = hit?.rects[0];
        if (hit && r) goTo(hit.page, Math.max(0, r.y0 - 60));
      },
    ),
  );

  return (
    <div class="viewer-host">
      <Show when={error()}>
        <p class="error center">{error()}</p>
      </Show>
      <div class="scroller" ref={scroller} tabIndex={0} onScroll={onScroll} onWheel={onWheel} onKeyDown={onKey} onCopy={onCopy} onMouseUp={onMouseUp}>
        <Show when={doc() && info()} fallback={<p class="muted center">{t("viewer.loading")}</p>}>
          <div class="canvas-area" style={{ width: `${Math.max(lay().width + margin(), view().w)}px`, height: `${lay().height}px` }}>
            <For each={shown()}>
              {(s) => (
                <div class="slot" classList={{ raised: (picked()?.paper === props.paper.id && find(props.paper.id, picked()?.id ?? "")?.page === s.page) || pending()?.annotation.page === s.page }} style={{ left: `${s.left}px`, top: `${s.top}px` }}>
                  <Page
                    paper={props.paper.id}
                    file={props.paper.file}
                    doc={doc() as PDFDocumentProxy}
                    page={s.page}
                    width={s.width}
                    height={s.height}
                    layoutScale={lay().scale}
                    renderScale={renderScale()}
                    priority={() => {
                      const mid = view().top + view().h / 2;
                      const d = Math.abs(s.top + s.height / 2 - mid);
                      return s.top + s.height >= view().top && s.top <= view().top + view().h ? 0 : d;
                    }}
                    colors={colors()}
                    text={texts[s.page]}
                    links={links[s.page]}
                    highlights={hitsOn(s.page)}
                    activeHighlight={activeRect(s.page)}
                    onInternalLink={(page, top) => goTo(page, top)}
                    onLinkHover={(target, at) => setPreview(target && at ? { ...target, at } : null)}
                    onExternalLink={(url) => void openExternal(url).catch((e: unknown) => setError(String(e)))}
                    flash={flashAt()?.page === s.page ? (flashAt()?.y ?? null) : null}
                  />
                  <Show when={margin() > 0}>
                    <MarginNotes paper={props.paper.id} page={s.page} scale={lay().scale} pageWidth={s.width} />
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
      <Show when={selBar() && props.active}>
        {(_) => (
          <SelectionBar
            at={selBar() as DOMRect}
            onHighlight={(c) => markSelection("highlight", c)}
            onMark={(k) => markSelection(k)}
            onComment={commentSelection}
            onCopy={() => {
              const text = selectedText((p) => texts[p]);
              if (text) void navigator.clipboard.writeText(text).catch(() => {});
              setSelBar(null);
            }}
            onCite={() => {
              api.copyCitation();
              setSelBar(null);
            }}
            onAddToNotes={() => {
              const ranges = selectionRanges((p) => texts[p]);
              const text = selectedText((p) => texts[p]);
              const first = ranges[0];
              const quads = first ? marksFor("highlight", [first], (p) => texts[p])[0]?.rect : undefined;
              window.getSelection()?.removeAllRanges();
              setSelBar(null);
              if (first && text) void quoteToNotes(props.paper.id, { text, page: first.page, y: quads?.y0 ?? 0 });
            }}
          />
        )}
      </Show>
      <Show when={preview() && doc()}>{(_) => <LinkPreview doc={doc() as PDFDocumentProxy} target={preview() as { page: number; top: number | null; at: DOMRect }} />}</Show>
    </div>
  );
}

/** A small view of where an internal link goes (P1.M2.VW.6 hover preview). */
function LinkPreview(props: { doc: PDFDocumentProxy; target: { page: number; top: number | null; at: DOMRect } }): JSX.Element {
  let canvas!: HTMLCanvasElement;
  const W = 360;
  const H = 200;
  onMount(async () => {
    const page = await props.doc.getPage(props.target.page);
    const base = page.getViewport({ scale: 1 });
    const s = W / base.width;
    const ratio = window.devicePixelRatio || 1;
    const top = Math.max(0, (props.target.top ?? 0) - 20) * s;
    canvas.width = W * ratio;
    canvas.height = H * ratio;
    const viewport = page.getViewport({ scale: s * ratio });
    await page.render({ canvas, viewport, annotationMode: 0, transform: [1, 0, 0, 1, 0, -top * ratio] }).promise.catch(() => {});
  });
  const left = Math.min(window.innerWidth - W - 16, props.target.at.left);
  const below = props.target.at.bottom + 8;
  const top = below + H > window.innerHeight ? props.target.at.top - H - 8 : below;
  return (
    <div class="link-preview" style={{ left: `${left}px`, top: `${top}px`, width: `${W}px`, height: `${H}px` }}>
      <canvas ref={canvas} style={{ width: `${W}px`, height: `${H}px` }} />
    </div>
  );
}
