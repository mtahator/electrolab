// Understanding papers by themselves (design language §17.12): started a moment after the app, when a
// paper is opened or added, and when a model appears. It runs in the background lane of every engine,
// gives way to whatever a person asks, waits on battery, and goes on where it stopped.

import { createSignal } from "solid-js";

import { showToast, t } from "@electropaper/kit";

import { onBusEvent } from "../host";
import { setSettingsOpen } from "../intel/state";
import { digestStatus, makeDigests, refreshDigests, stopDigests } from "./ask";
import { languageModels, refreshLanguageModels } from "./explain";

const KEY = "ep.understand";
const read = (): boolean => {
  try {
    return globalThis.localStorage?.getItem(KEY) !== "off";
  } catch {
    return true;
  }
};
const [on, setOnSignal] = createSignal(read());
export { on as understandOn };

/** Switch it on or off, from now on. */
export function setUnderstandOn(v: boolean): void {
  setOnSignal(v);
  try {
    globalThis.localStorage?.setItem(KEY, v ? "on" : "off");
  } catch {
    // For this session.
  }
  if (v) void makeDigests({ full: true });
  else void stopDigests();
}

/** Papers that were opened or added: they come next. Without papers: whatever is left. */
export function understand(papers: string[] = []): void {
  if (!on()) return;
  void makeDigests({ full: true, papers, auto: true }).catch(() => {});
}

/** Why it waits or ended, in words; nothing when it simply works. */
export function understandingNote(): string | null {
  const s = digestStatus();
  if (s?.waiting === "battery") return t("brain.battery");
  if (s?.running || !s?.error) return null;
  return s.error === "no_model" || s.error === "no_runtime" ? t("brain.no_model") : s.error;
}

const nameOf = (id: string): string => languageModels()?.models.find((m) => m.id === id)?.name ?? id;
let told = "";

/** Called once, when the window is there. */
export function startUnderstanding(): void {
  // After the first paper is on screen and the model was made ready.
  window.setTimeout(() => {
    void refreshDigests().catch(() => {});
    understand();
  }, 8000);
  void onBusEvent("inference.models_changed", () => understand());
  void onBusEvent("model.", async (e) => {
    void refreshLanguageModels();
    if (e.kind !== "model.fallback") return;
    const d = e.data as { wanted: string; used: string; reason: "not_enough_memory" | "start_failed" | "too_slow" };
    const key = `${d.wanted} ${d.used} ${d.reason}`;
    if (key === told) return;
    told = key;
    await refreshLanguageModels();
    // The person decides what the second choice is.
    showToast(t(`llm.fallback.${d.reason}`, { wanted: nameOf(d.wanted), used: nameOf(d.used) }), { label: t("llm.choose_order"), run: () => setSettingsOpen(true) });
  });
}
