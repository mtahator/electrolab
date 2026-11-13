import { createSignal } from "solid-js";

export type Theme = "light" | "dark" | "night" | "sepia";
export const THEMES: readonly Theme[] = ["light", "dark", "night", "sepia"];

const KEY = "ep.theme";

function initial(): Theme {
  const saved = globalThis.localStorage?.getItem(KEY);
  if (saved === "light" || saved === "dark" || saved === "night" || saved === "sepia") return saved;
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

const [theme, setThemeSignal] = createSignal<Theme>(initial());

/** Apply a theme now (live, no reload) and remember it. */
export function setTheme(t: Theme): void {
  setThemeSignal(t);
  document.documentElement.dataset.theme = t;
  globalThis.localStorage?.setItem(KEY, t);
}

export { theme };

/** Apply the remembered theme at start-up. */
export function applyTheme(): void {
  setTheme(theme());
}
