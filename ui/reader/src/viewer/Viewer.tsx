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
import { ExplainHost } from "../assistant/ExplainCard";
import { canExplain, explain } from "../assistant/explain";
import { SelectionBar } from "../annotations/SelectionBar";
import { type CharRange, marksFor, selectionBox, selectionRanges } from "../annotations/selection";
import { transformed } from "../annotations/AnnotationLayer";
import { add, change, find, isMarkup, load as loadAnnotations, pending, picked, remove, setPending, setPicked, setTool, tool, typing } from "../annotations/state";
import { bus, openExternal } from "../host";
import { PeekHost } from "../intel/Peek";
import { prepare } from "../intel/state";
import { quoteToNotes } from "../notes/state";
import { marginView } from "../shell/frame";
import { History, type Place } from "./history";
import { type Layout, type Slot, type ViewMode, type Zoom, clamp, layout, pageAt, scaleFor, toPage, toScreen, visible } from "./layout";
import { Page, selectedText } from "./Page";
import { type PDFDocumentProxy, acquire, release } from "./pdf";
import { loadPreviews, previewKey, previewsDone, previewsNear } from "./previews";

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
  /** Show a place and flash it, as soon as the pages are there (after the saved place). */
  openAt: (page: number, y: number) => void;
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
const reducedMotion = (): boolean => window.matchMedia("(prefers-reduced-motion: reduce)").matches;
/** Pages around the view whose text and links stay loaded. */
const KEEP_NEAR = 10;

/** Page colours of the paper setting (dark papers), or null for pages as printed. */
export function pageColors(): { background: string; foreground: string } | null {
  const css = getComputedStyle(document.documentElement);
  const ink = css.getPropertyValue("--page-ink").trim();
  const bg = css.getPropertyValue("--page-bg").trim();
  return ink ? { background: bg || "#000", foreground: ink } : null;
}


export function Viewer(props: ViewerProps): JSX.Element {
  let scroller!: HTMLDivElement;
  const hash = props.paper.file.hash;
  const [doc, setDoc] = createSignal<PDFDocumentProxy | null>(null);
  // PDF.js is loaded only when needed (recoloured papers, link previews): PDFium draws pages, so
  // the first page never waits for it.
  let docP: Promise<PDFDocumentProxy> | null = null;
  let stopPreviews = (): void => {};
  const getDoc = (): Promise<PDFDocumentProxy> =>
    (docP ??= acquire(hash).then((d) => {
      setDoc(d);
      return d;
    }));
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
  const [scrolling, setScrolling] = createSignal(false);
  let scrollIdle: number | undefined;
  let lastScroll = { top: 0, at: 0 };
  // Motion blur of previews (design language §7): vertical only, and only while the reader
  // flies through several pages — fast (about a page per 0.4 s or more) and far (more than a
  // page in this movement). Small or slow movements stay sharp so they can be read.
  const blurId = `ep-motion-${Math.random().toString(36).slice(2, 8)}`;
  let blurNow = 0;
  let samples: { at: number; dy: number }[] = []; // the last 100 ms of movement
  let flight = 0; // px travelled since the movement became fast
  const FAST = 2.5;
  const setBlur = (amount: number): void => {
    const node = document.getElementById(`${blurId}-fe`);
    if (amount === blurNow || !node) return;
    blurNow = amount;
    node.setAttribute("stdDeviation", `0 ${amount}`);
    scroller.style.setProperty("--preview-filter", amount > 0 ? `url(#${blurId})` : "none");
  };
  const motion = (dy: number, now: number): void => {
    if (dy === 0) return;
    samples.push({ at: now, dy: Math.abs(dy) });
    samples = samples.filter((x) => now - x.at <= 100);
    const span = Math.max(16, now - (samples[0]?.at ?? now) + 16);
    const speed = samples.reduce((a, x) => a + x.dy, 0) / span;
    flight = speed > FAST ? flight + Math.abs(dy) : 0;
    const pageH = lay().slots[0]?.height ?? scroller.clientHeight;
    setBlur(speed > FAST && flight > pageH ? Math.min(8, Math.round((speed - FAST) * 2) + 1) : 0);
  };
  // Where the scroll bar's thumb is, for the page label beside it (design language §17.5).
  const [thumb, setThumb] = createSignal({ y: 0, right: 14 });
  const [thumbDrag, setThumbDrag] = createSignal(false);
  const measureThumb = (): void => {
    const h = scroller.clientHeight;
    const total = Math.max(1, scroller.scrollHeight);
    const thumbH = Math.max(24, (h * h) / total);
    const range = Math.max(1, scroller.scrollHeight - h);
    const top = (scroller.scrollTop / range) * (h - thumbH);
    setThumb({ y: top + thumbH / 2, right: Math.max(8, scroller.offsetWidth - scroller.clientWidth) + 6 });
  };
  const [flashAt, setFlashAt] = createSignal<{ page: number; y: number; n: number } | null>(null);
  const margin = (): number => (marginView(props.paper.id) ? MARGIN_W : 0);
  const history = new History();
  const [histTick, setHistTick] = createSignal(0);
  let restored = false;
  let saveTimer: number | undefined;

  const sizes = (): DocInfo["sizes"] => info()?.sizes ?? [];
  // Only the size of the view, not where it is scrolled to: a layout makes new pages (a selection in
  // the old ones would be lost), so it may only change when a size does.
  const viewW = createMemo(() => view().w);
  const viewH = createMemo(() => view().h);
  const scale = createMemo(() => scaleFor(zoom(), sizes(), mode(), viewW() - margin(), viewH()));
  // Only the single-page layout depends on the current page; in the other layouts a page change
  // must not rebuild (and redraw) every page.
  const layoutPage = createMemo(() => (mode() === "single" ? current() : 1));
  const lay = createMemo<Layout>(() => layout(sizes(), scale(), mode(), viewW() - margin(), layoutPage()));
  // Pages mounted around the view: more ahead than behind, so pages are drawn before they arrive.
  const shown = createMemo(() => visible(lay(), view().top - view().h, view().top + 3 * view().h));
  // Pages are kept by their number: a new size (zoom, window, panel) moves and scales them, it does
  // not make them anew, so what is selected in them stays.
  const shownPages = createMemo(() => shown().map((s) => s.page));
  const slots = createMemo(() => new Map(lay().slots.map((s) => [s.page, s])));
  const slotOf = (page: number, old: Slot | undefined): Slot => slots().get(page) ?? old ?? { page, left: 0, top: 0, width: 0, height: 0 };
  // Page colours follow the paper setting (independent of the app theme).
  const colors = createMemo(() => {
    paper();
    return pageColors();
  });

  // Previews are made near where the reader is, and more as they move.
  createEffect(on(current, (p) => previewsNear(previewKey(hash, untrack(colors)), p), { defer: true }));

  // A different paper colour needs its own previews.
  createEffect(
    on(
      colors,
      (c) => {
        const i = info();
        if (!i) return;
        stopPreviews();
        stopPreviews = loadPreviews(props.paper.file, i.pages, current, c);
      },
      { defer: true },
    ),
  );

  // A saved place is applied when the layout of the pages exists (and the scroller has its size).
  const [restoreTo, setRestoreTo] = createSignal<Place | null>(null);
  createEffect(() => {
    const p = restoreTo();
    const l = lay();
    if (!p || l.slots.length === 0 || view().h < 50) return;
    setRestoreTo(null);
    queueMicrotask(() => goTo(p.page, p.y, false, true));
  });

  // A place asked for while the paper is still opening comes after the saved one.
  const [jumpTo, setJumpTo] = createSignal<Place | null>(null);
  const [placed, setPlaced] = createSignal(false);
  createEffect(() => {
    const p = jumpTo();
    if (!p || !placed() || restoreTo() || lay().slots.length === 0 || view().h < 50) return;
    setJumpTo(null);
    window.setTimeout(() => {
      goTo(p.page, Math.max(0, p.y - 60));
      api.flash(p.page, p.y);
    }, 30);
  });

  // Keep the reader's place when the page scale changes for another reason than zooming: the
  // window or a side panel changes size (fit width / fit page). Zooming keeps its own anchor.
  let place: Place | null = null;
  const zoomKey = (z: Zoom): string => (z.kind === "scale" ? `scale:${z.factor}` : z.kind);
  let lastZoom = zoomKey(zoom());
  createEffect(
    on(
      scale,
      () => {
        const z = zoomKey(zoom());
        const zoomed = z !== lastZoom;
        lastZoom = z;
        const p = place;
        if (zoomed || !p || !restored) return;
        queueMicrotask(() => goTo(p.page, p.y, false, true));
      },
      { defer: true },
    ),
  );

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
    if (!s) return { page, y: 0 };
    // In the gap below a page, the place is the top of the next page (a place past a page's
    // end would restore to a different page at another zoom).
    const next = l.slots.find((x) => x.page === page + 1);
    if (next && mode() !== "single" && view().top + 1 > s.top + s.height) return { page: page + 1, y: 0 };
    return { page, y: Math.max(0, (view().top - s.top) / l.scale) };
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
      if (!pt) return;
      const to = Math.max(0, pt.y - (exact ? 0 : y === null ? 16 : 40));
      // Nearby jumps glide (the eye can follow where it went); far ones are instant, since gliding
      // past many pages would only draw pages nobody reads.
      const near = Math.abs(to - scroller.scrollTop) < 2.5 * scroller.clientHeight;
      if (exact || !near || reducedMotion()) {
        scroller.scrollTop = to;
        // The page in view, even when the position did not change (no scroll event).
        if (mode() !== "single") setCurrent(pageAt(lay(), to + scroller.clientHeight / 2));
      }
      else {
        scroller.scrollTo({ top: to, behavior: "smooth" });
        // Save the place where the glide ends, not a page on the way.
        if (record) {
          window.clearTimeout(saveTimer);
          scroller.addEventListener("scrollend", () => void save(), { once: true });
        }
      }
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
    openAt: (page, y) => void setJumpTo({ page, y }),
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
  // Explain the selection (P1.M5.UI.1): the words, what stands around them, where they are.
  const explainSelection = (): void => {
    const box = selectionBox();
    const r = selectionRanges((p) => texts[p])[0];
    const pt = r && texts[r.page];
    const text = selectedText((p) => texts[p])?.trim();
    if (!box || !r || !pt || !text) return;
    const chars = Array.from(pt.text);
    void explain({
      paper: props.paper.id,
      file: props.paper.file,
      page: r.page,
      y: marksFor("highlight", [r], (p) => texts[p])[0]?.rect.y0 ?? 0,
      selection: text,
      before: chars.slice(Math.max(0, r.start - 500), r.start).join(""),
      after: chars.slice(r.end, r.end + 500).join(""),
      at: box,
    });
  };
  // Double-click explains a word — only when it can be answered (design language §11).
  const onDblClick = (e: MouseEvent): void => {
    if (tool() !== "select" || !canExplain() || !(e.target as HTMLElement).closest(".text-layer")) return;
    window.setTimeout(explainSelection, 0);
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
  // The selection as characters, drawn by the pages (design language §9.10), once per frame.
  const [selected, setSelected] = createSignal<CharRange[]>([]);
  onMount(() => {
    let frame = 0;
    const onSel = (): void => {
      const s = window.getSelection();
      if (!s || s.isCollapsed) setSelBar(null);
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setSelected(props.active ? selectionRanges((p) => texts[p]) : []));
    };
    document.addEventListener("selectionchange", onSel);
    onCleanup(() => {
      cancelAnimationFrame(frame);
      document.removeEventListener("selectionchange", onSel);
    });
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
      const [i, st] = await Promise.all([bus("document.info", { file: props.paper.file }), bus("workspace.get_reading_state", { paper: props.paper.id })]);
      setInfo(i);
      restore(st);
      stopPreviews = loadPreviews(props.paper.file, i.pages, current, untrack(colors));
      window.setTimeout(() => void getDoc().catch(() => {}), 1500);
      // Prepare the paper in the background (objects, links), starting with the page in view, once
      // the pages have their previews (both use the one PDF engine; drawing comes first), or after
      // a few seconds at most.
      const startPrep = (): void => void prepare(props.paper.file, [current()]);
      let prepStarted = false;
      const once = (): void => {
        if (!prepStarted) {
          prepStarted = true;
          startPrep();
        }
      };
      void previewsDone(previewKey(hash, untrack(colors))).then(once);
      window.setTimeout(once, 6000);
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
    stopPreviews();
    if (docP) release(hash);
  });

  const restore = (st: ReadingState): void => {
    if (st.view_mode) setModeSignal(st.view_mode);
    const z = st.zoom;
    if (z) setZoomSignal(z.kind === "scale" ? { kind: "scale", factor: z.factor } : { kind: z.kind });
    const loc = st.location;
    if (loc) {
      setCurrent(loc.page);
      place = { page: loc.page, y: loc.target?.y0 ?? 0 };
      // Applied once the pages are laid out at the real size (an effect below), not on a
      // timer: opening is fast now, and a place applied before the layout exists is lost.
      setRestoreTo(place);
    }
    restored = true;
    setPlaced(true);
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
    const now = performance.now();
    const dy = scroller.scrollTop - lastScroll.top;
    lastScroll = { top: scroller.scrollTop, at: now };
    motion(dy, now);
    measureThumb();
    setScrolling(true);
    window.clearTimeout(scrollIdle);
    scrollIdle = window.setTimeout(() => {
      setScrolling(false);
      samples = [];
      flight = 0;
      setBlur(0);
    }, 900);
    if (preview()) setPreview(null);
    setView((v) => ({ ...v, top: scroller.scrollTop }));
    place = here();
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
        // Soon, not "when idle": a busy app is never idle, and text tools need the text
        // (design language §17.3: selectable within 1.5 s).
        const idle =
          (window as Window & { requestIdleCallback?: (f: () => void, o?: { timeout: number }) => number }).requestIdleCallback ??
          ((f: () => void) => window.setTimeout(f, 30));
        idle(() => {
          void bus("document.text", { file: props.paper.file, page: p }).then((tx) => setTexts(p, tx)).catch(() => {});
        }, { timeout: 150 });
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

  // Touch (design language §17.8): two fingers pinch-zoom the page around them (the browser's
  // own pinch is off, see `touch-action`), a double tap toggles between fit width and 200 %.
  // The scroll bar thumb: pressing in the bar shows the page label beside the thumb.
  const touches = new Map<number, { x: number; y: number }>();
  let pinch: { dist: number; scale: number } | null = null;
  let pinchFrame: { factor: number; x: number; y: number } | null = null;
  let lastTap = { at: 0, x: 0, y: 0 };
  const spread = (): { dist: number; x: number; y: number } => {
    const [a, b] = [...touches.values()] as [{ x: number; y: number }, { x: number; y: number }];
    return { dist: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
  };
  const onPointerDown = (e: PointerEvent): void => {
    if (e.pointerType === "mouse" && e.offsetX >= scroller.clientWidth) {
      measureThumb();
      setThumbDrag(true);
      const up = (): void => {
        setThumbDrag(false);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointerup", up);
      return;
    }
    if (e.pointerType !== "touch") return;
    touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (touches.size === 2) {
      pinch = { dist: spread().dist, scale: scale() };
    } else if (touches.size === 1) {
      const now = performance.now();
      if (now - lastTap.at < 300 && Math.hypot(e.clientX - lastTap.x, e.clientY - lastTap.y) < 30) {
        const z = zoom();
        zoomAround(z.kind === "fit_width" ? { kind: "scale", factor: clamp(scale() * 2) } : { kind: "fit_width" }, e.clientX, e.clientY);
        lastTap = { at: 0, x: 0, y: 0 };
      } else {
        lastTap = { at: now, x: e.clientX, y: e.clientY };
      }
    }
  };
  const onPointerMove = (e: PointerEvent): void => {
    if (e.pointerType !== "touch" || !touches.has(e.pointerId)) return;
    touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (!pinch || touches.size !== 2) return;
    const s = spread();
    const start = pinch;
    const first = pinchFrame === null;
    pinchFrame = { factor: s.dist / start.dist, x: s.x, y: s.y };
    if (!first) return;
    requestAnimationFrame(() => {
      const f = pinchFrame;
      pinchFrame = null;
      if (f && pinch) zoomAround({ kind: "scale", factor: clamp(pinch.scale * f.factor) }, f.x, f.y);
    });
  };
  const onPointerEnd = (e: PointerEvent): void => {
    touches.delete(e.pointerId);
    if (touches.size < 2) pinch = null;
  };
  // A finger lifted outside the page still ends its touch.
  const endAnywhere = (e: PointerEvent): void => {
    if (e.pointerType === "touch") onPointerEnd(e);
  };
  window.addEventListener("pointerup", endAnywhere);
  window.addEventListener("pointercancel", endAnywhere);
  onCleanup(() => {
    window.removeEventListener("pointerup", endAnywhere);
    window.removeEventListener("pointercancel", endAnywhere);
  });

  // Ctrl + wheel and trackpad pinch (which arrives as Ctrl + wheel) zoom around the pointer.
  // Steps are gathered and applied once per frame, so a fast gesture lays the pages out once
  // per frame instead of once per event; pages are drawn sharp when the gesture ends.
  let wheelZoom: { factor: number; x: number; y: number } | null = null;
  const onWheel = (e: WheelEvent): void => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const step = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0025));
    if (wheelZoom) {
      wheelZoom.factor *= step;
      wheelZoom.x = e.clientX;
      wheelZoom.y = e.clientY;
      return;
    }
    wheelZoom = { factor: step, x: e.clientX, y: e.clientY };
    requestAnimationFrame(() => {
      const z = wheelZoom;
      wheelZoom = null;
      if (z) zoomAround({ kind: "scale", factor: clamp(scale() * z.factor) }, z.x, z.y);
    });
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
      else scroller.scrollBy({ top: dir * (h - 40), behavior: reducedMotion() ? "auto" : "smooth" });
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
      <svg class="motion-filter" aria-hidden="true" width="0" height="0">
        <filter id={blurId} x="0" y="-10%" width="100%" height="120%">
          <feGaussianBlur id={`${blurId}-fe`} stdDeviation="0 0" />
        </filter>
      </svg>
      <Show when={(scrolling() || thumbDrag()) && info()}>
        <div
          class="page-indicator"
          classList={{ dragging: thumbDrag() }}
          aria-hidden="true"
          style={{ top: `${thumb().y}px`, right: `${thumb().right}px` }}
        >
          {thumbDrag() ? t("viewer.page_indicator", { n: current(), total: info()?.pages ?? 0 }) : `${current()} / ${info()?.pages}`}
        </div>
      </Show>
      <div
        class="scroller"
        ref={scroller}
        tabIndex={0}
        onScroll={onScroll}
        onWheel={onWheel}
        onKeyDown={onKey}
        onCopy={onCopy}
        onMouseUp={onMouseUp}
        onDblClick={onDblClick}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd}
        onPointerCancel={onPointerEnd}
      >
        <Show when={info()} fallback={<p class="muted center">{t("viewer.loading")}</p>}>
          <div class="canvas-area" style={{ width: `${Math.max(lay().width + margin(), view().w)}px`, height: `${lay().height}px` }}>
            <For each={shownPages()}>
              {(p) => {
                const s = createMemo<Slot>((old) => slotOf(p, old));
                return (
                <div class="slot" classList={{ raised: (picked()?.paper === props.paper.id && find(props.paper.id, picked()?.id ?? "")?.page === p) || pending()?.annotation.page === p }} style={{ left: `${s().left}px`, top: `${s().top}px` }}>
                  <Page
                    paper={props.paper.id}
                    file={props.paper.file}
                    doc={getDoc}
                    page={p}
                    width={s().width}
                    height={s().height}
                    layoutScale={lay().scale}
                    renderScale={renderScale()}
                    priority={() => {
                      const mid = view().top + view().h / 2;
                      const d = Math.abs(s().top + s().height / 2 - mid);
                      return s().top + s().height >= view().top && s().top <= view().top + view().h ? 0 : d;
                    }}
                    colors={colors()}
                    text={texts[p]}
                    links={links[p]}
                    highlights={hitsOn(p)}
                    activeHighlight={activeRect(p)}
                    onInternalLink={(page, top) => goTo(page, top)}
                    onLinkHover={(target, at) => setPreview(target && at ? { ...target, at } : null)}
                    onExternalLink={(url) => void openExternal(url).catch((e: unknown) => setError(String(e)))}
                    selection={selected().find((r) => r.page === p) ?? null}
                    flash={flashAt()?.page === p ? (flashAt()?.y ?? null) : null}
                  />
                  <Show when={margin() > 0}>
                    <MarginNotes paper={props.paper.id} page={p} scale={lay().scale} pageWidth={s().width} />
                  </Show>
                </div>
                );
              }}
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
            onExplain={explainSelection}
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
      <Show when={props.active}>
        <PeekHost paper={props.paper.id} goTo={(page, y) => goTo(page, y)} />
        <ExplainHost
          paper={props.paper.id}
          goTo={(page, y) => {
            goTo(page, Math.max(0, y - 60));
            api.flash(page, y);
          }}
        />
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
