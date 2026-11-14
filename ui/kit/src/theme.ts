// App theme and paper colour (guideline §3.2). They are separate choices: the frame can be dark
// while pages keep their true colours, or the reverse. "Match the theme" pairs each theme with
// its own paper.

import { createSignal } from "solid-js";

export type Theme = "light" | "dark" | "night" | "sepia" | "contrast";
export const THEMES: readonly Theme[] = ["light", "dark", "night", "sepia", "contrast"];

/** How pages are shown. `original`: the PDF's true colours on white. `sepia`: warm paper, true
 *  ink colours. `dark`, `night`, `contrast`: recoloured text and lines (images keep theirs). */
export type Paper = "original" | "sepia" | "dark" | "night" | "contrast";
export const PAPERS: readonly Paper[] = ["original", "sepia", "dark", "night", "contrast"];
export type PaperChoice = Paper | "auto";

const KEY = "ep.theme";
const PAPER_KEY = "ep.paper";

/** The paper each theme uses when the paper is set to match the theme. */
export const THEME_PAPER: Record<Theme, Paper> = { light: "original", sepia: "sepia", dark: "dark", night: "night", contrast: "contrast" };

function initial(): Theme {
  const saved = globalThis.localStorage?.getItem(KEY);
  if (saved === "light" || saved === "dark" || saved === "night" || saved === "sepia" || saved === "contrast") return saved;
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function initialPaper(): PaperChoice {
  const saved = globalThis.localStorage?.getItem(PAPER_KEY);
  return saved && (PAPERS as readonly string[]).includes(saved) ? (saved as Paper) : "auto";
}

const [theme, setThemeSignal] = createSignal<Theme>(initial());
const [paperChoice, setPaperChoice] = createSignal<PaperChoice>(initialPaper());
/** The paper in use (the choice, or the theme's own paper). */
const [paper, setPaperSignal] = createSignal<Paper>(paperChoice() === "auto" ? THEME_PAPER[theme()] : (paperChoice() as Paper));

export { paper, paperChoice, theme };

function applyPaper(): void {
  const p = paperChoice() === "auto" ? THEME_PAPER[theme()] : (paperChoice() as Paper);
  // The attribute first: listeners of the signal read the new paper's CSS variables.
  document.documentElement.dataset.paper = p;
  setPaperSignal(p);
}

/** Apply a theme now (live, no reload) and remember it. */
export function setTheme(t: Theme): void {
  document.documentElement.dataset.theme = t;
  setThemeSignal(t);
  applyPaper();
  try {
    globalThis.localStorage?.setItem(KEY, t);
  } catch {
    // A convenience only.
  }
}

/** Choose the paper colour ("auto" matches the theme) and remember it. */
export function setPaper(p: PaperChoice): void {
  setPaperChoice(p);
  applyPaper();
  try {
    if (p === "auto") globalThis.localStorage?.removeItem(PAPER_KEY);
    else globalThis.localStorage?.setItem(PAPER_KEY, p);
  } catch {
    // A convenience only.
  }
}

/** Apply the remembered theme and paper at start-up. */
export function applyTheme(): void {
  setTheme(theme());
}
