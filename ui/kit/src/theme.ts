import { createSignal } from "solid-js";

export type Theme = "light" | "dark" | "night" | "sepia" | "contrast";
export const THEMES: readonly Theme[] = ["light", "dark", "night", "sepia", "contrast"];

const KEY = "ep.theme";

function initial(): Theme {
  const saved = globalThis.localStorage?.getItem(KEY);
  if (saved === "light" || saved === "dark" || saved === "night" || saved === "sepia" || saved === "contrast") return saved;
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

const [theme, setThemeSignal] = createSignal<Theme>(initial());

/** Apply a theme now (live, no reload) and remember it. */
export function setTheme(t: Theme): void {
  // The attribute first: listeners of the signal read the new theme's CSS variables.
  document.documentElement.dataset.theme = t;
  setThemeSignal(t);
  globalThis.localStorage?.setItem(KEY, t);
}

export { theme };

/** Apply the remembered theme at start-up. */
export function applyTheme(): void {
  setTheme(theme());
}
