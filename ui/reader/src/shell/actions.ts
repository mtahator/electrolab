// Every user action in one table (P1.M2.SH.6): the command palette lists them, the keymap binds
// them, and the keymap settings page lets the user change the keys.

import { bind } from "@electropaper/kit";
import { createSignal } from "solid-js";

export interface Action {
  id: string;
  title: () => string;
  /** Default keys, e.g. "Ctrl+O". */
  keys?: string;
  run: () => void;
  /** Hidden from the palette when this says so (e.g. no paper open). */
  enabled?: () => boolean;
}

const KEY = "ep.keymap";
const [actions, setActions] = createSignal<Action[]>([]);
const [overrides, setOverridesSignal] = createSignal<Record<string, string>>(load());
let unbinds: (() => void)[] = [];

function load(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, string>;
  } catch {
    return {};
  }
}

export { actions, overrides };

/** Keys currently bound to an action. */
export function keysOf(a: Action): string | undefined {
  return overrides()[a.id] ?? a.keys;
}

function rebind(): void {
  for (const u of unbinds) u();
  unbinds = [];
  for (const a of actions()) {
    const k = keysOf(a);
    if (k) unbinds.push(bind(k, () => (a.enabled?.() ?? true) && a.run()));
  }
}

/** Register the app's actions (once, at start-up). */
export function registerActions(list: Action[]): void {
  setActions(list);
  rebind();
}

/** Change (or reset, with `null`) the keys of an action. */
export function setKeys(id: string, keys: string | null): void {
  const o = { ...overrides() };
  if (keys) o[id] = keys;
  else delete o[id];
  setOverridesSignal(o);
  try {
    localStorage.setItem(KEY, JSON.stringify(o));
  } catch {
    // Custom keys are a convenience.
  }
  rebind();
}
