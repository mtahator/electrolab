// The window frame's state (guideline §2): which side panels are open, focus mode, margin view,
// and the bridge that lets panels move the focused viewer. Per-viewer conveniences are kept in
// browser storage; the app works the same without it.

import { createSignal } from "solid-js";

export type LeftTab = "outline" | "thumbnails" | "find" | "bookmarks" | "annotations";
export type RightTab = "notes";

function read<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

function keep(key: string, v: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    // A convenience only.
  }
}

const [leftPanel, setLeft] = createSignal<LeftTab | null>(read<LeftTab | null>("ep.left", "outline"));
const [rightPanel, setRight] = createSignal<RightTab | null>(read<RightTab | null>("ep.right", null));
const [leftWidth, setLeftW] = createSignal<number>(read("ep.left-width", 280));
const [rightWidth, setRightW] = createSignal<number>(read("ep.right-width", 380));
const [focusMode, setFocusMode] = createSignal(false);
const [margins, setMargins] = createSignal<Record<string, boolean>>(read("ep.margin", {}));

/** The paper keeps at least this much room (design language §2); the tool rail stands beside it. */
const PAPER = 320;
const RAIL = 44;
const crowded = (): boolean =>
  leftPanel() !== null && rightPanel() !== null && window.innerWidth - RAIL - leftWidth() - rightWidth() < PAPER;

/** When both panels do not fit, the one not just opened gives way (it is not forgotten: what was
 *  chosen stays stored for a larger window). */
function makeRoom(opened: "left" | "right"): void {
  if (!crowded()) return;
  if (opened === "left") setRight(null);
  else setLeft(null);
}

window.addEventListener("resize", () => makeRoom("left"));
makeRoom("left");

export { focusMode, leftPanel, leftWidth, rightPanel, rightWidth, setFocusMode };

export function setLeftPanel(tab: LeftTab | null): void {
  setLeft(tab);
  keep("ep.left", tab);
  makeRoom("left");
}

export function setRightPanel(tab: RightTab | null): void {
  setRight(tab);
  keep("ep.right", tab);
  makeRoom("right");
}

export function setLeftWidth(w: number): void {
  setLeftW(w);
  keep("ep.left-width", w);
}

export function setRightWidth(w: number): void {
  setRightW(w);
  keep("ep.right-width", w);
}

/** Margin view of sticky notes, remembered per paper (off by default). */
export function marginView(paper: string): boolean {
  return margins()[paper] ?? false;
}

export function toggleMarginView(paper: string): void {
  const next = { ...margins(), [paper]: !marginView(paper) };
  setMargins(next);
  keep("ep.margin", next);
}

/** Go to a place in a paper's open view, flashing the passage (set by the shell). */
type Navigate = (paper: string, page: number, y: number, annotation?: string) => void;
let navigate: Navigate = () => {};

export function onNavigate(f: Navigate): void {
  navigate = f;
}

export function goToPlace(paper: string, page: number, y: number, annotation?: string): void {
  navigate(paper, page, y, annotation);
}
