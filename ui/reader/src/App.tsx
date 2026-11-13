import type { Hit, Paper } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import { Button, THEMES, type Theme, setTheme, t, theme } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";
import { createStore } from "solid-js/store";

import { bus, onBusEvent, onDropFiles, onOpenFiles, openPaperExternally, pickPdfs, printPaper, takePendingFiles } from "./host";
import { importPdf, openDefaultLibrary, refresh } from "./library";
import { registerActions } from "./shell/actions";
import { LibraryView } from "./shell/LibraryView";
import { Confirm, KeymapSettings, Palette } from "./shell/Overlays";
import { activate, activeTab, closeTab, moveTab, openPapers, openTab, restore, session, setShowLibrary, toggleSplit, updatePaper } from "./shell/session";
import { type PanelKind, SidePanel } from "./shell/SidePanel";
import type { Marks, ViewerApi, ViewerStatus } from "./viewer/Viewer";
import { Viewer } from "./viewer/Viewer";

const ZOOM_STEPS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 5];
const PANELS: PanelKind[] = ["outline", "thumbnails", "find", "bookmarks"];

export function App(): JSX.Element {
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [dragging, setDragging] = createSignal(false);
  const [panel, setPanel] = createSignal<PanelKind | null>("outline");
  const [panelWidth, setPanelWidth] = createSignal(280);
  const [findFocus, setFindFocus] = createSignal(0);
  const [palette, setPalette] = createSignal(false);
  const [keymap, setKeymap] = createSignal(false);
  const [confirm, setConfirm] = createSignal<{ message: string; answer: (ok: boolean) => void } | null>(null);
  const [status, setStatus] = createStore<Record<string, ViewerStatus>>({});
  const [marks, setMarks] = createStore<Record<string, Marks>>({});
  const apis = new Map<string, ViewerApi>();

  const report = (e: unknown): void => {
    setError(e instanceof BusError ? e.message : String((e as Error)?.message ?? e));
  };
  const ask = (message: string): Promise<boolean> => new Promise((answer) => setConfirm({ message, answer }));
  const current = (): { api: ViewerApi | undefined; status: ViewerStatus | undefined; paper: Paper | null } => {
    const tab = activeTab();
    return { api: tab ? apis.get(tab.id) : undefined, status: tab ? status[tab.id] : undefined, paper: tab?.paper ?? null };
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

  const withPaper = (f: (p: Paper) => void) => (): void => {
    const p = current().paper;
    if (p) f(p);
  };

  registerActions([
    { id: "file.add", title: () => t("library.add"), keys: "Ctrl+O", run: () => void addPdfs() },
    { id: "view.library", title: () => t("library.title"), keys: "Ctrl+L", run: () => setShowLibrary(!session.showLibrary) },
    { id: "palette", title: () => t("palette.title"), keys: "Ctrl+K", run: () => setPalette(true) },
    { id: "keymap", title: () => t("keymap.title"), run: () => setKeymap(true) },
    {
      id: "find",
      title: () => t("panel.find"),
      keys: "Ctrl+F",
      run: () => {
        setPanel("find");
        setFindFocus((v) => v + 1);
      },
    },
    ...PANELS.filter((k) => k !== "find").map((k) => ({ id: `panel.${k}`, title: () => t(`panel.${k}`), run: () => setPanel(panel() === k ? null : k) })),
    {
      id: "bookmark.add",
      title: () => t("panel.add_bookmark"),
      keys: "Ctrl+D",
      enabled: () => !!current().paper,
      run: withPaper((paper) => {
        const page = current().status?.page ?? 1;
        void bus("workspace.add_bookmark", { name: "", location: { paper: paper.id, page } }).then(() => setPanel("bookmarks"), report);
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
    { id: "paper.print", title: () => t("viewer.print"), keys: "Ctrl+P", enabled: () => !!current().paper, run: withPaper((p) => void printPaper(p.file.hash).catch(report)) },
    ...THEMES.map((th) => ({ id: `theme.${th}`, title: () => `${t("settings.theme")}: ${t(`theme.${th}`)}`, run: () => setTheme(th) })),
  ]);

  onMount(async () => {
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

  const st = (): ViewerStatus | undefined => current().status;
  const zoomLabel = (): string => {
    const s = st();
    if (!s) return "";
    return s.zoom.kind === "fit_width" ? t("viewer.fit_width") : s.zoom.kind === "fit_page" ? t("viewer.fit_page") : `${Math.round(s.scale * 100)}%`;
  };

  const startResize = (e: PointerEvent): void => {
    const startX = e.clientX;
    const startW = panelWidth();
    const move = (m: PointerEvent): void => {
      setPanelWidth(Math.min(560, Math.max(180, startW + m.clientX - startX)));
    };
    const up = (): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div class="app">
      <header class="topbar">
        <strong>{t("app.title")}</strong>
        <Button onClick={() => setShowLibrary(!session.showLibrary)} title="Ctrl+L" data-pressed={session.showLibrary}>
          {t("library.title")}
        </Button>
        <Button variant="primary" disabled={busy()} onClick={() => void addPdfs()} title="Ctrl+O">
          {busy() ? t("library.adding") : t("library.add")}
        </Button>
        <Show when={!session.showLibrary && st()}>
          {(s) => (
            <div class="viewer-tools">
              <button type="button" class="icon" disabled={!s().canBack} title={`${t("viewer.back")} (Alt+Left)`} onClick={() => current().api?.back()}>
                ←
              </button>
              <button type="button" class="icon" disabled={!s().canForward} title={`${t("viewer.forward")} (Alt+Right)`} onClick={() => current().api?.forward()}>
                →
              </button>
              <label class="field">
                {t("viewer.page_box")}
                <input
                  class="ep-input page-box"
                  value={s().page}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") current().api?.goTo(Number(e.currentTarget.value) || 1, null);
                  }}
                  onFocus={(e) => e.currentTarget.select()}
                />
                / {s().pages}
              </label>
              <button type="button" class="icon" title={`${t("viewer.zoom_out")} (Ctrl+-)`} onClick={() => current().api?.zoomBy(1 / 1.2)}>
                −
              </button>
              <select
                class="ep-select"
                value=""
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  e.currentTarget.value = "";
                  if (v === "fit_width" || v === "fit_page") current().api?.setZoom({ kind: v });
                  else if (v) current().api?.setZoom({ kind: "scale", factor: Number(v) });
                }}
              >
                <option value="">{zoomLabel()}</option>
                <option value="fit_width">{t("viewer.fit_width")}</option>
                <option value="fit_page">{t("viewer.fit_page")}</option>
                <For each={ZOOM_STEPS}>{(z) => <option value={z}>{Math.round(z * 100)}%</option>}</For>
              </select>
              <button type="button" class="icon" title={`${t("viewer.zoom_in")} (Ctrl+=)`} onClick={() => current().api?.zoomBy(1.2)}>
                +
              </button>
              <select class="ep-select" value={s().mode} aria-label={t("viewer.mode")} onChange={(e) => current().api?.setMode(e.currentTarget.value as ViewerStatus["mode"])}>
                <option value="continuous">{t("viewer.mode_continuous")}</option>
                <option value="single">{t("viewer.mode_single")}</option>
                <option value="two_page">{t("viewer.mode_two_page")}</option>
              </select>
            </div>
          )}
        </Show>
        <span class="spacer" />
        <Show when={!session.showLibrary}>
          <div class="panel-tabs">
            <For each={PANELS}>
              {(k) => (
                <button type="button" class="panel-tab" classList={{ active: panel() === k }} onClick={() => setPanel(panel() === k ? null : k)}>
                  {t(`panel.${k}`)}
                </button>
              )}
            </For>
          </div>
          <Button onClick={toggleSplit} title="Ctrl+\">
            {session.panes.length > 1 ? t("tabs.unsplit") : t("tabs.split")}
          </Button>
        </Show>
        <label class="field">
          {t("settings.theme")}
          <select class="ep-select" value={theme()} onChange={(e) => setTheme(e.currentTarget.value as Theme)}>
            <For each={THEMES}>{(th) => <option value={th}>{t(`theme.${th}`)}</option>}</For>
          </select>
        </label>
        <Button onClick={() => setPalette(true)} title={`${t("palette.title")} (Ctrl+K)`}>
          {t("palette.title")}
        </Button>
      </header>
      <Show when={error()}>
        <div class="banner" role="alert">
          <strong>{t("error.title")}:</strong> {error()}
          <button type="button" class="icon" onClick={() => setError(null)} aria-label={t("common.close")}>
            ×
          </button>
        </div>
      </Show>
      <Show
        when={!session.showLibrary}
        fallback={<LibraryView onOpen={openTab} onAdd={() => void addPdfs()} onError={report} confirm={(name) => ask(t("confirm.delete_collection", { name }))} />}
      >
        <div class="workspace">
          <Show when={panel()}>
            {(k) => (
              <>
                <aside class="side" style={{ width: `${panelWidth()}px` }}>
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
                </aside>
                <div class="resizer" onPointerDown={startResize} />
              </>
            )}
          </Show>
          <div class="panes">
            <For each={session.panes}>
              {(pane, pi) => (
                <section
                  class="pane"
                  classList={{ focused: session.focused === pi() }}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    const data = e.dataTransfer?.getData("application/x-ep-tab");
                    if (data) {
                      const { pane: from, id } = JSON.parse(data) as { pane: number; id: string };
                      moveTab(from, id, pi());
                    }
                  }}
                >
                  <div class="tab-strip">
                    <For each={pane.tabs}>
                      {(tab) => (
                        <div
                          class="tab"
                          classList={{ active: pane.active === tab.id }}
                          draggable={true}
                          onDragStart={(e) => e.dataTransfer?.setData("application/x-ep-tab", JSON.stringify({ pane: pi(), id: tab.id }))}
                          onClick={() => activate(pi(), tab.id)}
                          title={tab.paper.title ?? tab.paper.original_name}
                        >
                          <span class="tab-title">{tab.paper.title ?? tab.paper.original_name}</span>
                          <button
                            type="button"
                            class="icon"
                            aria-label={t("tabs.close")}
                            onClick={(e) => {
                              e.stopPropagation();
                              apis.delete(tab.id);
                              closeTab(pi(), tab.id);
                            }}
                          >
                            ×
                          </button>
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
                  </div>
                </section>
              )}
            </For>
          </div>
        </div>
      </Show>
      <Show when={dragging()}>
        <div class="drop-overlay">{t("library.drop_here")}</div>
      </Show>
      <Show when={palette()}>
        <Palette onClose={() => setPalette(false)} />
      </Show>
      <Show when={keymap()}>
        <KeymapSettings onClose={() => setKeymap(false)} />
      </Show>
      <Show when={confirm()}>
        {(c) => (
          <Confirm
            message={c().message}
            onAnswer={(ok) => {
              setConfirm(null);
              c().answer(ok);
            }}
          />
        )}
      </Show>
    </div>
  );
}
