// The one search of the Reader (design language §17.4). The find bar (Ctrl+F, the search
// button) and the Find list in the side panel are two views of the same search: one query,
// one set of options, one result, one current hit. Results stream in while pages are searched
// (§17.1), pages near the one in view first.

import type { Hit, Paper, SearchResult } from "@electropaper/bindings";
import { createStore } from "solid-js/store";

import { bus, onBusEvent } from "../host";

export interface FindOptions {
  query: string;
  mode: "word_forms" | "exact" | "fuzzy";
  caseSensitive: boolean;
  wholeWord: boolean;
  allTabs: boolean;
}

interface FindState extends FindOptions {
  hits: Hit[];
  /** Every match (the hits list may be cut); null before a search. */
  total: number | null;
  active: number | null;
  /** Pages searched so far while the search runs; null when done. */
  progress: { done: number; total: number } | null;
  error: string | null;
}

const [state, setState] = createStore<FindState>({
  query: "",
  mode: "word_forms",
  caseSensitive: false,
  wholeWord: true,
  allTabs: false,
  hits: [],
  total: null,
  active: null,
  progress: null,
  error: null,
});

/** The search, for reading (both views render from it). */
export const find = state;

/** Hits shown in the pages at most (the count stays exact). */
const MAX_HITS = 5000;

interface Context {
  paper: () => Paper | null;
  openPapers: () => Paper[];
  page: () => number;
  onHits: (hits: Hit[], active: number | null) => void;
}
let ctx: Context | null = null;
let streams = 0;
let currentId = "";
let timer: number | undefined;

/** The app tells the search where it is (current paper, open papers, page, where hits go). */
export function bindFind(c: Context): void {
  ctx = c;
}

const show = (r: SearchResult, final: boolean): void => {
  let active = state.active;
  if (active === null && r.hits.length > 0) {
    // The first hit at or after the page in view becomes the current one.
    const page = ctx?.page() ?? 1;
    const i = r.hits.findIndex((h) => h.page >= page);
    active = i >= 0 ? i : 0;
  }
  setState({
    hits: r.hits,
    total: r.counts.reduce((a, c) => a + c.total, 0),
    progress: final ? null : { done: r.pages_searched ?? 0, total: r.pages_total ?? 0 },
    active,
    error: null,
  });
  ctx?.onHits(r.hits, active);
};

void onBusEvent("document.searching", (e) => {
  const d = e.data as { id: string; result: SearchResult };
  if (d.id === currentId) show(d.result, false);
}).catch(() => {});

async function run(): Promise<void> {
  streams += 1;
  const id = `find-${streams}`;
  currentId = id;
  const papers = state.allTabs ? (ctx?.openPapers() ?? []) : ctx?.paper() ? [ctx.paper() as Paper] : [];
  setState({ active: null });
  if (!state.query.trim() || papers.length === 0) {
    setState({ hits: [], total: null, progress: null, error: null });
    ctx?.onHits([], null);
    return;
  }
  // Answer at once: the search shows as running before the first page is read.
  setState({ total: 0, hits: [], progress: { done: 0, total: 0 }, error: null });
  try {
    const r = await bus("document.search", {
      papers: papers.map((p) => p.file.hash),
      terms: [{ text: state.query.trim() }],
      match: { mode: state.mode, case_sensitive: state.caseSensitive, whole_word: state.wholeWord },
      context: { unit: "words", before: 8, after: 8 },
      limits: { max_hits: MAX_HITS, max_hits_per_term: MAX_HITS, max_context_chars: 2_000_000 },
      near_page: ctx?.page() ?? 1,
      stream: { id, group: "find" },
    });
    if (currentId === id) show(r, true);
  } catch (e) {
    // A newer search stopped this one; anything else is shown.
    if (currentId === id) setState({ progress: null, error: String((e as Error)?.message ?? e) });
  }
}

/** Change the query or options; the search runs shortly after typing stops. */
export function setFind(patch: Partial<FindOptions>): void {
  setState(patch);
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void run(), 150);
}

/** Next (1) or previous (-1) hit. */
export function stepFind(d: number): void {
  const n = state.hits.length;
  if (n === 0) return;
  const next = ((state.active ?? -1) + d + n) % n;
  setState({ active: next });
  ctx?.onHits(state.hits, next);
}

/** Go to hit `i`. */
export function pickHit(i: number): void {
  setState({ active: i });
  ctx?.onHits(state.hits, i);
}

/** Show the hits in the pages again (the find bar or list opened). */
export function showFindMarks(): void {
  ctx?.onHits(state.hits, state.active);
}

/** Take the hits off the pages (both views closed); the query stays for next time. */
export function hideFindMarks(): void {
  ctx?.onHits([], null);
}
