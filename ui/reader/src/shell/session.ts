// Open tabs and panes (P1.M2.SH.1), restored on the next start. The layout is a per-viewer
// convenience, so it lives in browser storage; the papers themselves live in the library.

import type { Paper } from "@electropaper/bindings";
import { createStore, produce } from "solid-js/store";

export interface Tab {
  id: string;
  paper: Paper;
}

export interface Pane {
  tabs: Tab[];
  active: string | null;
}

export interface Session {
  panes: Pane[];
  focused: number;
  showLibrary: boolean;
}

const KEY = "ep.session";
let nextId = 1;
const newId = (): string => `t${Date.now().toString(36)}${(nextId++).toString(36)}`;

const [session, setSession] = createStore<Session>({ panes: [{ tabs: [], active: null }], focused: 0, showLibrary: true });

export { session };

/** The active tab of the focused pane. */
export function activeTab(): Tab | null {
  const pane = session.panes[session.focused];
  return pane?.tabs.find((t) => t.id === pane.active) ?? null;
}

/** Every open paper (for "find in all open papers"). */
export function openPapers(): Paper[] {
  const seen = new Map<string, Paper>();
  for (const p of session.panes) for (const t of p.tabs) seen.set(t.paper.id, t.paper);
  return [...seen.values()];
}

/** Open a paper in the focused pane (or switch to its tab if already open there). */
export function openTab(paper: Paper, pane = session.focused): void {
  setSession(
    produce((s) => {
      const target = s.panes[pane] ?? s.panes[0];
      if (!target) return;
      const existing = target.tabs.find((t) => t.paper.id === paper.id);
      if (existing) target.active = existing.id;
      else {
        const tab = { id: newId(), paper };
        target.tabs.push(tab);
        target.active = tab.id;
      }
      s.focused = Math.min(pane, s.panes.length - 1);
      s.showLibrary = false;
    }),
  );
  persist();
}

export function closeTab(pane: number, id: string): void {
  setSession(
    produce((s) => {
      const p = s.panes[pane];
      if (!p) return;
      const i = p.tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      p.tabs.splice(i, 1);
      if (p.active === id) p.active = p.tabs[Math.min(i, p.tabs.length - 1)]?.id ?? null;
      if (p.tabs.length === 0 && s.panes.length > 1) {
        s.panes.splice(pane, 1);
        s.focused = 0;
      }
      if (s.panes.every((x) => x.tabs.length === 0)) s.showLibrary = true;
    }),
  );
  persist();
}

export function activate(pane: number, id: string): void {
  setSession(
    produce((s) => {
      const p = s.panes[pane];
      if (p) p.active = id;
      s.focused = pane;
      s.showLibrary = false;
    }),
  );
  persist();
}

/** Move a tab to another pane (drag and drop between panes). */
export function moveTab(from: number, id: string, to: number): void {
  setSession(
    produce((s) => {
      const src = s.panes[from];
      const dst = s.panes[to];
      if (!src || !dst || from === to) return;
      const i = src.tabs.findIndex((t) => t.id === id);
      const [tab] = i >= 0 ? src.tabs.splice(i, 1) : [];
      if (!tab) return;
      dst.tabs.push(tab);
      dst.active = tab.id;
      if (src.active === id) src.active = src.tabs[0]?.id ?? null;
      s.focused = to;
      if (src.tabs.length === 0) {
        s.panes.splice(from, 1);
        s.focused = 0;
      }
    }),
  );
  persist();
}

/** Split into two panes (the active tab moves to the new pane) or join back into one. */
export function toggleSplit(): void {
  setSession(
    produce((s) => {
      if (s.panes.length === 2) {
        const [a, b] = s.panes;
        if (!a || !b) return;
        a.tabs.push(...b.tabs);
        s.panes = [a];
        s.focused = 0;
        return;
      }
      const a = s.panes[0];
      if (!a) return;
      const tab = a.tabs.find((t) => t.id === a.active);
      if (tab && a.tabs.length > 1) {
        a.tabs = a.tabs.filter((t) => t.id !== tab.id);
        a.active = a.tabs[0]?.id ?? null;
        s.panes.push({ tabs: [tab], active: tab.id });
      } else {
        s.panes.push({ tabs: [], active: null });
      }
      s.focused = 1;
    }),
  );
  persist();
}

export function setShowLibrary(v: boolean): void {
  setSession("showLibrary", v);
  persist();
}

/** Refresh a paper's record in every tab (after its metadata or tags change). */
export function updatePaper(paper: Paper): void {
  setSession(
    produce((s) => {
      for (const p of s.panes) for (const t of p.tabs) if (t.paper.id === paper.id) t.paper = paper;
    }),
  );
}

interface Saved {
  panes: { papers: string[]; active: number }[];
  focused: number;
  showLibrary: boolean;
}

function persist(): void {
  const saved: Saved = {
    panes: session.panes.map((p) => ({ papers: p.tabs.map((t) => t.paper.id), active: Math.max(0, p.tabs.findIndex((t) => t.id === p.active)) })),
    focused: session.focused,
    showLibrary: session.showLibrary,
  };
  try {
    localStorage.setItem(KEY, JSON.stringify(saved));
  } catch {
    // Session restore is a convenience.
  }
}

/** Restore the last session; `load` fetches a paper by id (missing papers are skipped). */
export async function restore(load: (id: string) => Promise<Paper | null>): Promise<void> {
  let saved: Saved | null = null;
  try {
    saved = JSON.parse(localStorage.getItem(KEY) ?? "null") as Saved | null;
  } catch {
    saved = null;
  }
  if (!saved) return;
  const panes: Pane[] = [];
  for (const p of saved.panes.slice(0, 2)) {
    const tabs: Tab[] = [];
    for (const id of p.papers) {
      const paper = await load(id);
      if (paper) tabs.push({ id: newId(), paper });
    }
    if (tabs.length > 0 || panes.length === 0) panes.push({ tabs, active: tabs[Math.min(p.active, tabs.length - 1)]?.id ?? null });
  }
  setSession({ panes, focused: Math.min(saved.focused, panes.length - 1), showLibrary: saved.showLibrary || panes.every((p) => p.tabs.length === 0) });
}
