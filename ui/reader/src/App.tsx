// The Reader window (guideline §2): one-row top bar, tool rail, left panel (navigation), the
// panes with the document, right panel (notes), status line; focus mode shows only the page.

import type { Hit, Paper } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import {
  Banner,
  Button,
  Dialog,
  IconButton,
  type MenuItem,
  Panel,
  StatusItem,
  StatusLine,
  PAPERS,
  TEXT_SIZES,
  THEMES,
  density,
  openMenu,
  paperChoice,
  setDensity,
  setPaper,
  setTextSize,
  setTheme,
  showToast,
  t,
  textSize,
  theme,
} from "@electropaper/kit";
import { For, type JSX, Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { createStore } from "solid-js/store";

import { ToolRail, toolHint } from "./annotations/ToolRail";
import { annotationsOf, marksHidden, onAnnotationError, picked, redo, remove, saveIntoPdf, savedAt, setMarksHidden, setPicked, setTool, tool, typing, undo } from "./annotations/state";
import { bus, downloadPdf, onBusEvent, onDropFiles, onOpenFiles, openPaperExternally, pickPdfs, takePendingFiles } from "./host";
import { addTextLayer, canAddTextLayer, download, fetchModel, installChoice, layoutModel, prepare, refreshModels, running, setSettingsOpen as setSettings, settingsOpen as settings } from "./intel/state";
import { importPdf, openDefaultLibrary, refresh } from "./library";
import { NotesPanel } from "./notes/NotesPanel";
import { flushAll, onNotesError } from "./notes/state";
import { registerActions } from "./shell/actions";
import {
  type LeftTab,
  focusMode,
  leftPanel,
  leftWidth,
  marginView,
  onNavigate,
  rightPanel,
  rightWidth,
  setFocusMode,
  setLeftPanel,
  setLeftWidth,
  setRightPanel,
  setRightWidth,
  toggleMarginView,
} from "./shell/frame";
import { LibraryView } from "./shell/LibraryView";
import { KeymapSettings, Palette, Prompt } from "./shell/Overlays";
import { activate, activeTab, closeTab, moveTab, openPapers, openTab, restore, session, setShowLibrary, toggleSplit, updatePaper } from "./shell/session";
import { PrintSheet } from "./shell/Print";
import { explainCard, refreshLanguageModels } from "./assistant/explain";
import { Settings } from "./shell/Settings";
import { SidePanel } from "./shell/SidePanel";
import type { Marks, ViewerApi, ViewerStatus } from "./viewer/Viewer";
import { bindFind, hideFindMarks } from "./viewer/find";
import { FindBar } from "./viewer/FindBar";
import { Viewer } from "./viewer/Viewer";

const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];
const LEFT_TABS: { id: LeftTab; icon: Parameters<typeof IconButton>[0]["icon"]; label: () => string; shortcut?: string }[] = [
  { id: "outline", icon: "list-tree", label: () => t("panel.outline") },
  { id: "thumbnails", icon: "layout-grid", label: () => t("panel.thumbnails") },
  { id: "find", icon: "search", label: () => t("panel.find"), shortcut: "Ctrl+F" },
  { id: "bookmarks", icon: "bookmark", label: () => t("panel.bookmarks") },
  { id: "annotations", icon: "list-checks", label: () => t("panel.annotations") },
];
const TOOL_KEYS = [
  ["select", "V"],
  ["highlight", "H"],
  ["underline", "U"],
  ["strike_out", "S"],
  ["note", "N"],
  ["free_text", "T"],
  ["ink", "D"],
  ["rectangle", "R"],
  ["ellipse", "O"],
  ["line", "L"],
  ["arrow", "A"],
  ["eraser", "E"],
  ["squiggly", ""],
] as const;

export function App(): JSX.Element {
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [dragging, setDragging] = createSignal(false);
  const [findFocus, setFindFocus] = createSignal(0);
  const [palette, setPalette] = createSignal(false);
  const [keymap, setKeymap] = createSignal(false);
  const [confirm, setConfirm] = createSignal<{ message: string; answer: (ok: boolean) => void } | null>(null);
  const [urlPrompt, setUrlPrompt] = createSignal(false);
  const [savingPdf, setSavingPdf] = createSignal(false);
  const [showSaved, setShowSaved] = createSignal(false);
  const [peek, setPeek] = createSignal(false);
  const [modelDialog, setModelDialog] = createSignal(false);
  const [addingText, setAddingText] = createSignal(false);
  // The find bar over the focused pane (design language §17.4).
  const [findBar, setFindBar] = createSignal(0);
  // Printing prints the paper through the Reader's print sheet and the standard print dialog
  // (design language §17.9): it needs no other PDF app.
  const [printing, setPrinting] = createSignal<Paper | null>(null);
  const printPdf = (p: Paper): void => {
    setPrinting(p);
  };
  // Share of the first pane in split view (design language §17.6), remembered.
  const readSplit = (): number => {
    try {
      const v = Number(globalThis.localStorage?.getItem("ep.pane-split"));
      return v >= 0.15 && v <= 0.85 ? v : 0.5;
    } catch {
      return 0.5;
    }
  };
  const [split, setSplit] = createSignal(readSplit());
  const saveSplit = (v: number): void => {
    setSplit(v);
    try {
      globalThis.localStorage?.setItem("ep.pane-split", String(v));
    } catch {
      // A convenience.
    }
  };
  let panesBox: HTMLDivElement | undefined;
  const dragSplit = (e: PointerEvent): void => {
    const box = panesBox?.getBoundingClientRect();
    if (!box) return;
    const handle = e.currentTarget as HTMLElement;
    handle.classList.add("dragging");
    const min = Math.min(0.45, 280 / box.width);
    const move = (m: PointerEvent): void => saveSplit(Math.min(1 - min, Math.max(min, (m.clientX - box.left) / box.width)));
    const up = (): void => {
      handle.classList.remove("dragging");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const [note, setNote] = createSignal<string | null>(null);
  const [status, setStatus] = createStore<Record<string, ViewerStatus>>({});
  const [marks, setMarks] = createStore<Record<string, Marks>>({});
  const apis = new Map<string, ViewerApi>();

  const report = (e: unknown): void => {
    setError(e instanceof BusError ? e.message : String((e as Error)?.message ?? e));
  };
  onAnnotationError(report);
  onNotesError(report);
  const ask = (message: string): Promise<boolean> => new Promise((answer) => setConfirm({ message, answer }));
  const current = (): { api: ViewerApi | undefined; status: ViewerStatus | undefined; paper: Paper | null } => {
    const tab = activeTab();
    return { api: tab ? apis.get(tab.id) : undefined, status: tab ? status[tab.id] : undefined, paper: tab?.paper ?? null };
  };
  const reading = (): boolean => !session.showLibrary && !!current().paper;

  // "Saved" shows for two seconds after each change (guideline §9.3).
  createEffect(
    on(savedAt, (at) => {
      if (!at) return;
      setShowSaved(true);
      const id = window.setTimeout(() => setShowSaved(false), 2000);
      onCleanup(() => window.clearTimeout(id));
    }),
  );

  // Save into PDF (Ctrl+S): the paper gets a new managed copy; open tabs keep showing the same
  // pages (annotations are drawn from the store), so nothing reloads.
  const savePdf = async (paper: Paper): Promise<void> => {
    setSavingPdf(true);
    try {
      const updated = await saveIntoPdf(paper.id);
      if (updated) updatePaper(updated);
    } finally {
      setSavingPdf(false);
    }
  };

  // Scanned pages: write the recognised text into a new version of the PDF (P1.M4.OCR.2).
  const addText = async (paper: Paper): Promise<void> => {
    setAddingText(true);
    try {
      const r = await addTextLayer(paper);
      updatePaper(r.paper);
      setNote(t("intel.added_text_layer", { pages: r.pages }));
      window.setTimeout(() => setNote(null), 3000);
    } catch (e) {
      report(e);
    } finally {
      setAddingText(false);
    }
  };

  // The one search (design language §17.4): the find bar and the Find list share it.
  bindFind({
    paper: () => current().paper,
    openPapers: () => openPapers(),
    page: () => current().status?.page ?? 1,
    onHits: (hits, active) => showHits(hits, active),
  });
  const openFindBar = (): void => {
    if (!current().paper) return openFind();
    setFindBar((n) => n + 1);
  };
  const closeFindBar = (): void => {
    setFindBar(0);
    if (leftPanel() !== "find") hideFindMarks();
    current().api?.focus();
  };

  async function importAndOpen(paths: string[]): Promise<void> {
    if (paths.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const opened: Paper[] = [];
      for (const f of paths) opened.push(await importPdf(f));
      await refresh();
      for (const p of opened) openTab(p);
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  }
  const addPdfs = async (): Promise<void> => importAndOpen(await pickPdfs(t("library.add")));

  // Open a PDF from a web address (P1.M2.SH.7): download, add with the address kept, open.
  async function openUrl(url: string): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const path = await downloadPdf(url);
      const paper = await importPdf(path, url);
      await refresh();
      openTab(paper);
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  }

  // Find hits: give each open tab the hits in its paper; bring the active hit's tab forward.
  const showHits = (hits: Hit[], active: number | null): void => {
    for (const pane of session.panes) {
      for (const tab of pane.tabs) {
        const mine = hits.map((h, i) => ({ h, i })).filter(({ h }) => h.paper === tab.paper.file.hash);
        const idx = mine.findIndex(({ i }) => i === active);
        setMarks(tab.id, { hits: mine.map(({ h }) => ({ page: h.page, rects: h.rects })), active: idx >= 0 ? idx : null });
      }
    }
    const a = active === null ? undefined : hits[active];
    if (a) {
      session.panes.forEach((pane, pi) => {
        const tab = pane.tabs.find((tb) => tb.paper.file.hash === a.paper);
        if (tab && pane.active !== tab.id) activate(pi, tab.id);
      });
    }
  };

  // Location chips in notes go back to the place: the paper's tab comes forward, the passage
  // flashes once, a quoted mark opens its card.
  onNavigate((paper, page, y, annotation) => {
    let found: { pane: number; tab: string } | null = null;
    session.panes.forEach((pane, pi) => {
      const tab = pane.tabs.find((tb) => tb.paper.id === paper);
      if (tab && !found) found = { pane: pi, tab: tab.id };
    });
    const f = found as { pane: number; tab: string } | null;
    if (!f) return;
    activate(f.pane, f.tab);
    queueMicrotask(() => {
      const api = apis.get(f.tab);
      api?.goTo(page, Math.max(0, y - 60));
      if (annotation) setPicked({ paper, id: annotation, edit: true });
      else api?.flash(page, y);
    });
  });

  const withPaper = (f: (p: Paper) => void) => (): void => {
    const p = current().paper;
    if (p) f(p);
  };
  const toggleLeft = (tab: LeftTab): void => setLeftPanel(leftPanel() === tab ? null : tab);
  const openFind = (): void => {
    setLeftPanel("find");
    setFindFocus((v) => v + 1);
  };

  registerActions([
    { id: "file.add", title: () => t("library.add"), keys: "Ctrl+O", run: () => void addPdfs() },
    { id: "file.url", title: () => t("library.open_url"), keys: "Ctrl+Shift+O", run: () => setUrlPrompt(true) },
    { id: "view.library", title: () => t("library.title"), keys: "Ctrl+L", run: () => setShowLibrary(!session.showLibrary) },
    { id: "palette", title: () => t("palette.title"), keys: "Ctrl+K", run: () => setPalette(true) },
    { id: "keymap", title: () => t("keymap.title"), run: () => setKeymap(true) },
    { id: "settings", title: () => t("settings.title"), keys: "Ctrl+,", run: () => setSettings(true) },
    { id: "find", title: () => t("panel.find"), keys: "Ctrl+F", run: () => openFindBar() },
    { id: "find.list", title: () => `${t("panel.find")}: ${t("panel.title_list")}`, keys: "Ctrl+Shift+F", run: openFind },
    { id: "panel.left", title: () => t("frame.left_panel"), keys: "Ctrl+B", enabled: () => !typing(), run: () => setLeftPanel(leftPanel() ? null : "outline") },
    ...LEFT_TABS.filter((x) => x.id !== "find").map((x) => ({ id: `panel.${x.id}`, title: x.label, run: () => toggleLeft(x.id) })),
    { id: "panel.notes", title: () => t("panel.notes"), keys: "Ctrl+Shift+N", run: () => setRightPanel(rightPanel() ? null : "notes") },
    { id: "view.focus", title: () => t("frame.focus_mode"), keys: "F11", run: () => setFocusMode(!focusMode()) },
    { id: "view.focus2", title: () => t("frame.focus_mode"), keys: "Ctrl+.", run: () => setFocusMode(!focusMode()) },
    {
      id: "bookmark.add",
      title: () => t("panel.add_bookmark"),
      keys: "Ctrl+D",
      enabled: () => !!current().paper,
      run: withPaper((paper) => {
        const page = current().status?.page ?? 1;
        void bus("workspace.add_bookmark", { name: "", location: { paper: paper.id, page } }).then(() => setLeftPanel("bookmarks"), report);
      }),
    },
    { id: "view.split", title: () => (session.panes.length > 1 ? t("tabs.unsplit") : t("tabs.split")), keys: "Ctrl+\\", run: toggleSplit },
    {
      id: "tab.close",
      title: () => t("tabs.close"),
      keys: "Ctrl+W",
      enabled: () => !!activeTab(),
      run: () => {
        const tab = activeTab();
        if (tab) closeTab(session.focused, tab.id);
      },
    },
    {
      id: "tab.next",
      title: () => t("tabs.next"),
      keys: "Ctrl+Tab",
      run: () => {
        const pane = session.panes[session.focused];
        if (!pane || pane.tabs.length === 0) return;
        const i = pane.tabs.findIndex((tb) => tb.id === pane.active);
        const next = pane.tabs[(i + 1) % pane.tabs.length];
        if (next) activate(session.focused, next.id);
      },
    },
    { id: "nav.back", title: () => t("viewer.back"), enabled: () => !!current().api, run: () => current().api?.back() },
    { id: "nav.forward", title: () => t("viewer.forward"), enabled: () => !!current().api, run: () => current().api?.forward() },
    { id: "zoom.width", title: () => t("viewer.fit_width"), enabled: () => !!current().api, run: () => current().api?.setZoom({ kind: "fit_width" }) },
    { id: "zoom.page", title: () => t("viewer.fit_page"), enabled: () => !!current().api, run: () => current().api?.setZoom({ kind: "fit_page" }) },
    { id: "mode.continuous", title: () => t("viewer.mode_continuous"), enabled: () => !!current().api, run: () => current().api?.setMode("continuous") },
    { id: "mode.single", title: () => t("viewer.mode_single"), enabled: () => !!current().api, run: () => current().api?.setMode("single") },
    { id: "mode.two_page", title: () => t("viewer.mode_two_page"), enabled: () => !!current().api, run: () => current().api?.setMode("two_page") },
    { id: "paper.external", title: () => t("viewer.open_external"), enabled: () => !!current().paper, run: withPaper((p) => void openPaperExternally(p.file.hash).catch(report)) },
    { id: "paper.print", title: () => t("viewer.print"), keys: "Ctrl+P", enabled: () => !!current().paper, run: withPaper((p) => printPdf(p)) },
    { id: "annot.undo", title: () => t("annot.undo"), keys: "Ctrl+Z", enabled: () => !!current().paper && !typing(), run: withPaper((p) => void undo(p.id)) },
    { id: "annot.redo", title: () => t("annot.redo"), keys: "Ctrl+Y", enabled: () => !!current().paper && !typing(), run: withPaper((p) => void redo(p.id)) },
    { id: "annot.redo2", title: () => t("annot.redo"), keys: "Ctrl+Shift+Z", enabled: () => !!current().paper && !typing(), run: withPaper((p) => void redo(p.id)) },
    { id: "annot.save_pdf", title: () => t("annot.save_pdf"), keys: "Ctrl+S", enabled: () => !!current().paper && !savingPdf(), run: withPaper((p) => void savePdf(p)) },
    {
      id: "intel.text_layer",
      title: () => t("intel.add_text_layer"),
      enabled: () => !!current().paper && canAddTextLayer(current().paper?.file.hash ?? "") && !addingText(),
      run: withPaper((p) => void addText(p)),
    },
    { id: "annot.hide", title: () => (marksHidden() ? t("annot.show_all") : t("annot.hide_all")), keys: "Ctrl+H", run: () => setMarksHidden(!marksHidden()) },
    { id: "notes.margin", title: () => t("notes.margin_view"), keys: "Ctrl+Shift+M", enabled: () => !!current().paper, run: withPaper((p) => toggleMarginView(p.id)) },
    {
      id: "annot.delete",
      title: () => t("annot.delete"),
      keys: "Delete",
      enabled: () => !!picked() && !typing(),
      run: () => {
        const p = picked();
        if (p) void remove(p.paper, p.id);
      },
    },
    { id: "annot.copy_cite", title: () => t("annot.copy_cite"), keys: "Ctrl+Shift+C", enabled: () => !!current().api, run: () => void current().api?.copyCitation() },
    ...TOOL_KEYS.map(([tl, key]) => ({
      id: `tool.${tl}`,
      title: () => `${t("annot.tools")}: ${t(tl === "select" ? "annot.tool.select" : tl === "eraser" ? "annot.tool.eraser" : `annot.kind.${tl}`)}`,
      ...(key ? { keys: key } : {}),
      enabled: () => !!current().paper && !typing(),
      run: () => setTool(tl),
    })),
    ...THEMES.map((th) => ({ id: `theme.${th}`, title: () => `${t("settings.theme")}: ${t(`theme.${th}`)}`, run: () => setTheme(th) })),
    { id: "paper.auto", title: () => `${t("settings.paper")}: ${t("paper.auto")}`, run: () => setPaper("auto") },
    ...PAPERS.map((p) => ({ id: `paper.${p}`, title: () => `${t("settings.paper")}: ${t(`paper.${p}`)}`, run: () => setPaper(p) })),
    { id: "view.density", title: () => t("settings.compact"), run: () => setDensity(density() === "compact" ? "comfortable" : "compact") },
  ]);

  // Prepare the open papers again, now with a new model.
  const prepareAll = (): void => {
    for (const pane of session.panes) for (const tab of pane.tabs) void prepare(tab.paper.file, [status[tab.id]?.page ?? 1]);
  };

  // Ctrl+P is the Reader's print (the paper), never the browser's print of the interface.
  onMount(() => {
    const stopBrowserPrint = (e: KeyboardEvent): void => {
      if (!((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "p")) return;
      e.preventDefault();
      e.stopPropagation();
      const p = current().paper;
      if (p) printPdf(p);
    };
    window.addEventListener("keydown", stopBrowserPrint, true);
    onCleanup(() => window.removeEventListener("keydown", stopBrowserPrint, true));
  });

  onMount(async () => {
    void refreshLanguageModels();
    void refreshModels().then(async () => {
      // The user chose the layout model while installing: get it now, progress in the status line.
      if ((await installChoice()) && !layoutModel()?.present) void fetchModel("egret-medium").then((ok) => ok && prepareAll());
    });
    try {
      await openDefaultLibrary();
      await restore(async (id) => bus("workspace.get_paper", { id }).catch(() => null));
      await importAndOpen(await takePendingFiles());
    } catch (e) {
      report(e);
    }
    const offs = await Promise.all([
      onBusEvent("workspace.paper", (e) => {
        if (e.kind === "workspace.paper_changed") updatePaper(e.data as Paper);
        void refresh();
      }),
      onOpenFiles((paths) => void importAndOpen(paths)),
      onDropFiles((paths) => void importAndOpen(paths), setDragging),
    ]);
    onCleanup(() => offs.forEach((off) => off()));
  });
  onCleanup(() => void flushAll());

  const st = (): ViewerStatus | undefined => current().status;
  const zoomLabel = (): string => {
    const s = st();
    return s ? `${Math.round(s.scale * 100)} %` : "";
  };

  const zoomMenu = (at: DOMRect): void => {
    const z = st()?.zoom;
    openMenu(
      t("viewer.zoom"),
      [
        { label: t("viewer.fit_width"), checked: z?.kind === "fit_width", run: () => current().api?.setZoom({ kind: "fit_width" }) },
        { label: t("viewer.fit_page"), checked: z?.kind === "fit_page", run: () => current().api?.setZoom({ kind: "fit_page" }) },
        { separator: true },
        ...ZOOM_STEPS.map((f) => ({ label: `${Math.round(f * 100)} %`, checked: z?.kind === "scale" && Math.abs(z.factor - f) < 0.001, run: () => current().api?.setZoom({ kind: "scale", factor: f }) })),
      ],
      at,
    );
  };

  // App theme and paper colour are separate (e.g. a dark app with pages in their true colours).
  const themeMenu = (at: DOMRect): void =>
    openMenu(
      t("settings.theme"),
      [
        { heading: t("settings.app_theme") },
        ...THEMES.map((th) => ({ label: t(`theme.${th}`), checked: theme() === th, run: () => setTheme(th) })),
        { separator: true },
        { heading: t("settings.paper") },
        { label: t("paper.auto"), checked: paperChoice() === "auto", run: () => setPaper("auto") },
        ...PAPERS.map((p) => ({ label: t(`paper.${p}`), checked: paperChoice() === p, run: () => setPaper(p) })),
      ],
      at,
    );

  const moreMenu = (at: DOMRect): void => {
    const s = st();
    const p = current().paper;
    const items: MenuItem[] = [];
    if (s && p) {
      items.push(
        { heading: t("viewer.mode") },
        { label: t("viewer.mode_continuous"), checked: s.mode === "continuous", run: () => current().api?.setMode("continuous") },
        { label: t("viewer.mode_single"), checked: s.mode === "single", run: () => current().api?.setMode("single") },
        { label: t("viewer.mode_two_page"), checked: s.mode === "two_page", run: () => current().api?.setMode("two_page") },
        { separator: true },
        { label: t("frame.focus_mode"), icon: "maximize-2", shortcut: "F11", run: () => setFocusMode(true) },
        { label: marksHidden() ? t("annot.show_all") : t("annot.hide_all"), icon: marksHidden() ? "eye" : "eye-off", shortcut: "Ctrl+H", run: () => setMarksHidden(!marksHidden()) },
        { label: t("notes.margin_view"), icon: "messages-square", shortcut: "Ctrl+Shift+M", checked: marginView(p.id), run: () => toggleMarginView(p.id) },
        { label: t("annot.save_pdf"), icon: "file-down", shortcut: "Ctrl+S", disabled: !annotationsOf(p.id)?.unsaved, run: () => void savePdf(p) },
        ...(canAddTextLayer(p.file.hash) ? [{ label: t("intel.add_text_layer"), icon: "scan-text" as const, disabled: addingText(), run: () => void addText(p) }] : []),
        { separator: true },
        { label: t("viewer.open_external"), icon: "external-link", run: () => void openPaperExternally(p.file.hash).catch(report) },
        { label: t("viewer.print"), icon: "printer", shortcut: "Ctrl+P", run: () => printPdf(p) },
        { separator: true },
      );
    }
    items.push(
      { label: t("settings.compact"), checked: density() === "compact", run: () => setDensity(density() === "compact" ? "comfortable" : "compact") },
      { heading: t("settings.text_size") },
      ...TEXT_SIZES.map((n) => ({ label: `${n} %`, checked: textSize() === n, run: () => setTextSize(n) })),
      { separator: true },
      { label: t("keymap.title"), icon: "keyboard", run: () => setKeymap(true) },
      { label: t("settings.title"), icon: "settings", shortcut: "Ctrl+,", run: () => setSettings(true) },
    );
    openMenu(t("frame.more"), items, at);
  };

  const resize = (e: PointerEvent, side: "left" | "right"): void => {
    const startX = e.clientX;
    const startW = side === "left" ? leftWidth() : rightWidth();
    const move = (m: PointerEvent): void => {
      const d = side === "left" ? m.clientX - startX : startX - m.clientX;
      const w = Math.min(side === "left" ? 480 : 560, Math.max(side === "left" ? 220 : 300, startW + d));
      if (side === "left") setLeftWidth(w);
      else setRightWidth(w);
    };
    const up = (): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const unsaved = (): boolean => !!(current().paper && annotationsOf(current().paper?.id ?? "")?.unsaved);

  return (
    <div class="app" classList={{ "focus-mode": focusMode(), peek: focusMode() && peek() }}>
      <Show when={focusMode()}>
        <div class="peek-zone" onPointerEnter={() => setPeek(true)} />
      </Show>
      <header class="topbar" onPointerLeave={() => setPeek(false)}>
        <div class="bar-group">
          <IconButton icon="library" label={t("library.title")} shortcut="Ctrl+L" pressed={session.showLibrary} onClick={() => setShowLibrary(!session.showLibrary)} />
          <IconButton icon="panel-left" label={t("frame.left_panel")} shortcut="Ctrl+B" pressed={!!leftPanel()} disabled={session.showLibrary} onClick={() => setLeftPanel(leftPanel() ? null : "outline")} />
          <span class="bar-sep" />
          <IconButton icon="file-plus" label={busy() ? t("library.adding") : t("library.add")} shortcut="Ctrl+O" disabled={busy()} onClick={() => void addPdfs()} />
          <IconButton icon="link" label={t("library.open_url")} shortcut="Ctrl+Shift+O" disabled={busy()} onClick={() => setUrlPrompt(true)} />
        </div>
        <span class="ep-spacer" />
        <Show when={reading() && st()}>
          {(s) => (
            <div class="bar-group viewer-tools">
              <IconButton icon="arrow-left" label={t("viewer.back")} shortcut="Alt+←" disabled={!s().canBack} onClick={() => current().api?.back()} />
              <IconButton icon="arrow-right" label={t("viewer.forward")} shortcut="Alt+→" disabled={!s().canForward} onClick={() => current().api?.forward()} />
              <label class="page-field">
                <input
                  class="ep-input page-box"
                  aria-label={t("viewer.page_box")}
                  value={s().page}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") current().api?.goTo(Number(e.currentTarget.value) || 1, null);
                  }}
                  onFocus={(e) => e.currentTarget.select()}
                />
                <span class="muted">/ {s().pages}</span>
              </label>
              <span class="bar-sep" />
              <IconButton icon="zoom-out" label={t("viewer.zoom_out")} shortcut="Ctrl+−" onClick={() => current().api?.zoomBy(1 / 1.2)} />
              <button type="button" class="zoom-button" aria-label={`${t("viewer.zoom")}: ${zoomLabel()}`} data-tip={t("viewer.zoom")} onClick={(e) => zoomMenu(e.currentTarget.getBoundingClientRect())}>
                {zoomLabel()}
              </button>
              <IconButton icon="zoom-in" label={t("viewer.zoom_in")} shortcut="Ctrl+=" onClick={() => current().api?.zoomBy(1.2)} />
            </div>
          )}
        </Show>
        <span class="ep-spacer" />
        <div class="bar-group">
          <Show when={!session.showLibrary}>
            <IconButton icon="search" label={t("panel.find")} shortcut="Ctrl+F" pressed={findBar() > 0} onClick={() => (findBar() > 0 ? closeFindBar() : openFindBar())} />
            <IconButton icon="columns-2" label={session.panes.length > 1 ? t("tabs.unsplit") : t("tabs.split")} shortcut="Ctrl+\" pressed={session.panes.length > 1} onClick={toggleSplit} />
            <IconButton icon="notebook-pen" label={t("panel.notes")} shortcut="Ctrl+Shift+N" pressed={rightPanel() === "notes"} disabled={!current().paper} onClick={() => setRightPanel(rightPanel() ? null : "notes")} />
            <span class="bar-sep" />
          </Show>
          <IconButton icon="sun-moon" label={t("settings.theme")} onClick={(e) => themeMenu(e.currentTarget.getBoundingClientRect())} />
          <IconButton icon="command" label={t("palette.title")} shortcut="Ctrl+K" onClick={() => setPalette(true)} />
          <IconButton icon="ellipsis" label={t("frame.more")} onClick={(e) => moreMenu(e.currentTarget.getBoundingClientRect())} />
        </div>
      </header>
      <Show when={error()}>
        <Banner onClose={() => setError(null)} closeLabel={t("common.close")}>
          {error()}
        </Banner>
      </Show>
      <Show
        when={!session.showLibrary}
        fallback={<LibraryView
            onOpen={openTab}
            onAdd={() => void addPdfs()}
            onError={report}
            confirm={(name) => ask(t("confirm.delete_collection", { name }))}
            onRemoved={(p) =>
              session.panes.forEach((pane, pi) => pane.tabs.filter((tb) => tb.paper.id === p.id).forEach((tb) => closeTab(pi, tb.id)))
            }
          />}
      >
        <div class="frame">
          <Show when={current().paper}>{(p) => <ToolRail paper={p().id} saving={savingPdf()} onSave={() => void savePdf(p())} />}</Show>
          <Show when={leftPanel()}>
            {(k) => (
              <>
                <div class="side left" style={{ width: `${leftWidth()}px` }}>
                  <Panel
                    side="left"
                    label={t("frame.left_panel")}
                    tabs={LEFT_TABS.map((x) => ({ id: x.id, icon: x.icon, label: x.label(), shortcut: x.shortcut }))}
                    active={k()}
                    onTab={(id) => (id === "find" ? openFind() : setLeftPanel(id as LeftTab))}
                    onClose={() => setLeftPanel(null)}
                    closeLabel={t("common.close")}
                  >
                    <SidePanel
                      kind={k()}
                      paper={current().paper}
                      pages={st()?.pages ?? 0}
                      current={st()?.page ?? 1}
                      openPapers={openPapers()}
                      onGoTo={(page, y) => current().api?.goTo(page, y)}
                      onHits={showHits}
                      findFocus={findFocus()}
                    />
                  </Panel>
                </div>
                <div class="resizer" role="separator" aria-orientation="vertical" onPointerDown={(e) => resize(e, "left")} />
              </>
            )}
          </Show>
          <div class="panes" ref={panesBox}>
            <For each={session.panes}>
              {(pane, pi) => (
                <>
                <Show when={pi() > 0}>
                  <div
                    class="pane-resizer"
                    role="separator"
                    aria-orientation="vertical"
                    aria-label={t("frame.resize")}
                    data-tip={t("frame.resize")}
                    onPointerDown={dragSplit}
                    onDblClick={() => saveSplit(0.5)}
                  />
                </Show>
                <section
                  class="pane"
                  classList={{ focused: session.focused === pi() }}
                  style={session.panes.length > 1 ? { flex: `0 0 calc(${(pi() === 0 ? split() : 1 - split()) * 100}% - 3px)` } : {}}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    const data = e.dataTransfer?.getData("application/x-ep-tab");
                    if (data) {
                      const { pane: from, id } = JSON.parse(data) as { pane: number; id: string };
                      moveTab(from, id, pi());
                    }
                  }}
                >
                  <div class="tab-strip" role="tablist">
                    <For each={pane.tabs}>
                      {(tab) => (
                        <div
                          class="tab"
                          role="tab"
                          aria-selected={pane.active === tab.id}
                          classList={{ active: pane.active === tab.id }}
                          draggable={true}
                          onDragStart={(e) => e.dataTransfer?.setData("application/x-ep-tab", JSON.stringify({ pane: pi(), id: tab.id }))}
                          onClick={() => activate(pi(), tab.id)}
                          data-tip={tab.paper.title ?? tab.paper.original_name}
                        >
                          <span class="tab-title">{tab.paper.title ?? tab.paper.original_name}</span>
                          <IconButton
                            icon="x"
                            size="sm"
                            label={t("tabs.close")}
                            class="tab-close"
                            onClick={(e) => {
                              e.stopPropagation();
                              apis.delete(tab.id);
                              closeTab(pi(), tab.id);
                            }}
                          />
                        </div>
                      )}
                    </For>
                  </div>
                  <div class="pane-body" onPointerDown={() => pane.active && session.focused !== pi() && activate(pi(), pane.active)}>
                    <Show when={pane.tabs.find((tb) => tb.id === pane.active)} keyed fallback={<p class="muted center">{t("viewer.empty")}</p>}>
                      {(tab) => (
                        <Viewer
                          paper={tab.paper}
                          active={session.focused === pi()}
                          marks={marks[tab.id] ?? { hits: [], active: null }}
                          onReady={(api) => apis.set(tab.id, api)}
                          onStatus={(s) => setStatus(tab.id, s)}
                        />
                      )}
                    </Show>
                    <Show when={findBar() > 0 && session.focused === pi() && pane.tabs.find((tb) => tb.id === pane.active)}>
                      <Show when={findBar()} keyed>
                        {(_) => <FindBar onList={openFind} onClose={closeFindBar} />}
                      </Show>
                    </Show>
                  </div>
                </section>
                </>
              )}
            </For>
          </div>
          <Show when={rightPanel() && current().paper}>
            <div class="resizer" role="separator" aria-orientation="vertical" onPointerDown={(e) => resize(e, "right")} />
            <div class="side right" style={{ width: `${rightWidth()}px` }}>
              <Panel
                side="right"
                label={t("frame.right_panel")}
                tabs={[{ id: "notes", icon: "notebook-pen", label: t("panel.notes"), shortcut: "Ctrl+Shift+N" }]}
                active="notes"
                onTab={() => setRightPanel("notes")}
                onClose={() => setRightPanel(null)}
                closeLabel={t("common.close")}
              >
                <NotesPanel paper={current().paper as Paper} />
              </Panel>
            </div>
          </Show>
        </div>
      </Show>
      <StatusLine
        left={
          <>
            <Show when={busy()}>
              <StatusItem icon="file-plus">{t("library.adding")}</StatusItem>
            </Show>
            <Show when={savingPdf()}>
              <StatusItem icon="file-down">{t("annot.saving_pdf")}</StatusItem>
            </Show>
            <Show when={showSaved() && !savingPdf()}>
              <StatusItem icon="check">{t("annot.saved")}</StatusItem>
            </Show>
            <Show when={reading() && unsaved() && !savingPdf()}>
              <StatusItem icon="file-down" tone="accent" label={t("annot.save_pdf")} onClick={() => current().paper && void savePdf(current().paper as Paper)}>
                {t("annot.not_in_pdf")}
              </StatusItem>
            </Show>
            <Show when={reading() && current().paper && running(current().paper?.file.hash ?? "")}>
              {(r) => (
                <StatusItem icon="cpu">
                  {t("intel.preparing", { stage: t(`intel.stage.${r().stage}`), done: r().done, total: r().total })}
                </StatusItem>
              )}
            </Show>
            <Show when={explainCard()?.phase === "starting"}>
              <StatusItem icon="sparkles">{t("explain.starting")}</StatusItem>
            </Show>
            <Show when={addingText()}>
              <StatusItem icon="scan-text">{t("intel.adding_text_layer")}</StatusItem>
            </Show>
            <Show when={note()}>
              <StatusItem icon="check">{note()}</StatusItem>
            </Show>
            <Show when={download()}>
              {(d) => (
                <Show
                  when={!d().error}
                  fallback={
                    <StatusItem icon="circle-alert" tone="danger" label={t("intel.model_failed")} onClick={() => setModelDialog(true)}>
                      {t("intel.model_failed")}
                    </StatusItem>
                  }
                >
                  <StatusItem icon="cpu">{t("intel.downloading", { percent: d().total ? Math.round((100 * d().received) / d().total) : 0 })}</StatusItem>
                </Show>
              )}
            </Show>
            <Show when={reading() && !download() && layoutModel() && !layoutModel()?.present}>
              <StatusItem icon="cpu" label={t("intel.get_model")} onClick={() => setModelDialog(true)}>
                {t("intel.get_model")}
              </StatusItem>
            </Show>
            <Show when={reading() && tool() !== "select"}>
              <StatusItem icon="mouse-pointer-2">
                {toolHint(tool())} · {t("annot.stop_hint")}
              </StatusItem>
            </Show>
            <Show when={reading() && marksHidden()}>
              <StatusItem icon="eye-off" label={t("annot.show_all")} onClick={() => setMarksHidden(false)}>
                {t("annot.hidden")}
              </StatusItem>
            </Show>
          </>
        }
        right={
          <Show when={reading() && st()}>
            {(s) => (
              <>
                <StatusItem>{t("viewer.page", { n: s().page, total: s().pages })}</StatusItem>
                <StatusItem>{zoomLabel()}</StatusItem>
              </>
            )}
          </Show>
        }
      />
      <Show when={focusMode() && st()}>
        {(s) => (
          <div class="focus-indicator" aria-hidden="true">
            {s().page} / {s().pages}
          </div>
        )}
      </Show>
      <Show when={dragging()}>
        <div class="drop-overlay">{t("library.drop_here")}</div>
      </Show>
      <Show when={palette()}>
        <Palette onClose={() => setPalette(false)} />
      </Show>
      <Show when={printing()}>
        {(p) => <PrintSheet paper={p()} pages={st()?.pages ?? p().pages ?? 1} current={st()?.page ?? 1} onClose={() => setPrinting(null)} />}
      </Show>
      <Show when={settings()}>
        <Settings onClose={() => setSettings(false)} onModelReady={prepareAll} />
      </Show>
      <Show when={keymap()}>
        <KeymapSettings onClose={() => setKeymap(false)} />
      </Show>
      <Show when={urlPrompt()}>
        <Prompt
          message={t("library.open_url")}
          placeholder="https://"
          onAnswer={(url) => {
            setUrlPrompt(false);
            if (url) void openUrl(url);
          }}
        />
      </Show>
      <Show when={modelDialog()}>
        <Dialog
          title={t("intel.model_title")}
          onCancel={() => setModelDialog(false)}
          actions={
            <>
              <Button onClick={() => setModelDialog(false)}>{t("intel.not_now")}</Button>
              <Button
                variant="primary"
                icon="file-down"
                onClick={() => {
                  setModelDialog(false);
                  void fetchModel("egret-medium").then((ok) => ok && prepareAll());
                }}
              >
                {t("intel.download", { mb: Math.round((layoutModel()?.size ?? 0) / 1e6) })}
              </Button>
            </>
          }
        >
          <p>{t("intel.model_body")}</p>
          <Show when={download()?.error}>
            <p class="error">{download()?.error}</p>
          </Show>
        </Dialog>
      </Show>
      <Show when={confirm()}>
        {(c) => (
          <Dialog
            title={c().message}
            onCancel={() => {
              const { answer } = c();
              setConfirm(null);
              answer(false);
            }}
            actions={
              <>
                <Button
                  onClick={() => {
                    const { answer } = c();
                    setConfirm(null);
                    answer(false);
                  }}
                >
                  {t("common.cancel")}
                </Button>
                <Button
                  variant="danger"
                  data-autofocus
                  onClick={() => {
                    const { answer } = c();
                    setConfirm(null);
                    answer(true);
                  }}
                >
                  {t("common.delete")}
                </Button>
              </>
            }
          >
            {t("confirm.papers_stay")}
          </Dialog>
        )}
      </Show>
    </div>
  );
}
