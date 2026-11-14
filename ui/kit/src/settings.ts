// Interface settings shared by every app (guideline §2 density, §4 text size). Per-viewer
// conveniences, so they live in browser storage; the app works the same without it.

import { createSignal } from "solid-js";

export type Density = "comfortable" | "compact";
export const TEXT_SIZES = [90, 100, 110, 125, 150] as const;
export type TextSize = (typeof TEXT_SIZES)[number];

function read<T>(key: string, ok: (v: string) => boolean, fallback: T): T {
  try {
    const v = globalThis.localStorage?.getItem(key);
    return v !== null && v !== undefined && ok(v) ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, v: unknown): void {
  try {
    globalThis.localStorage?.setItem(key, JSON.stringify(v));
  } catch {
    // Settings are a convenience.
  }
}

const [density, setDensitySignal] = createSignal<Density>(read("ep.density", (v) => v === '"compact"' || v === '"comfortable"', "comfortable"));
const [textSize, setTextSizeSignal] = createSignal<TextSize>(read("ep.text-size", (v) => (TEXT_SIZES as readonly number[]).includes(Number(v)), 100));

export { density, textSize };

export function setDensity(d: Density): void {
  document.documentElement.dataset.density = d;
  setDensitySignal(d);
  write("ep.density", d);
}

export function setTextSize(s: TextSize): void {
  // Every text size token is in rem, so one root size scales the whole interface's text.
  document.documentElement.style.fontSize = `${s}%`;
  setTextSizeSignal(s);
  write("ep.text-size", s);
}

/** Apply the remembered settings at start-up. */
export function applySettings(): void {
  setDensity(density());
  setTextSize(textSize());
}
