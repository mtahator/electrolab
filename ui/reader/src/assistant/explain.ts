// Explain a selection (P1.M5.UI.1): one card at a time, streamed; closing it or Stop cancels the
// answer. The engine drafts, then checks the draft against the paper (understanding.explain_span).

import { type BlobRef, BusError, type Explanation, type LlmModels } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus, cancelCall, onBusEvent } from "../host";

export interface ExplainSelection {
  paper: string;
  file: BlobRef;
  page: number;
  /** Top of the selection on its page, for notes. */
  y: number;
  selection: string;
  before: string;
  after: string;
  /** Where the selection is on screen. */
  at: DOMRect;
}

export type Phase = "starting" | "draft" | "checking" | "done" | "stopped" | "failed";

export interface ExplainCardState extends ExplainSelection {
  id: string;
  phase: Phase;
  text: string;
  result?: Explanation;
  error?: { kind: string; message: string };
}

const [card, setCard] = createSignal<ExplainCardState | null>(null);
const [models, setModels] = createSignal<LlmModels | null>(null);
export { card as explainCard, models as languageModels };

export async function refreshLanguageModels(): Promise<void> {
  setModels(await bus("inference.models", null).catch(() => null));
}

/** Whether an answer is possible on this machine now (a model file and a runtime are here). */
export const canExplain = (): boolean => {
  const m = models();
  return !!m && m.runtime.backend !== "none" && m.models.some((x) => x.kind === "chat");
};

/** Search the model folders again. */
export async function searchModels(): Promise<void> {
  setModels(await bus("inference.search_models", null));
}

export async function addModelFolder(path: string): Promise<void> {
  setModels(await bus("inference.add_folder", { path }));
}

export async function removeModelFolder(path: string): Promise<void> {
  setModels(await bus("inference.remove_folder", { path }));
}

/** The model of a task (null: the default again). */
export async function assignModel(task: string, model: string | null): Promise<void> {
  setModels(await bus("inference.assign", model ? { task, model } : { task }));
}

const isRunning = (p: Phase): boolean => p === "starting" || p === "draft" || p === "checking";
export const explainRunning = (): boolean => isRunning(card()?.phase ?? "done");
const patch = (id: string, change: Partial<ExplainCardState>): void => {
  setCard((c) => (c && c.id === id ? { ...c, ...change } : c));
};

void onBusEvent("understanding.explaining", (e) => {
  const d = e.data as { id: string; phase: string; text: string };
  const c = card();
  if (!c || c.id !== d.id || !isRunning(c.phase)) return;
  patch(d.id, { text: d.text, phase: d.phase === "draft" ? "draft" : "checking" });
});

export function stopExplain(): void {
  const c = card();
  if (!c || !isRunning(c.phase)) return;
  patch(c.id, { phase: "stopped" });
  void cancelCall(c.id);
}

export function closeExplain(): void {
  stopExplain();
  setCard(null);
}

export async function explain(s: ExplainSelection): Promise<void> {
  closeExplain();
  const id = crypto.randomUUID();
  setCard({ ...s, id, phase: "starting", text: "" });
  try {
    const result = await bus(
      "understanding.explain_span",
      { file: s.file, page: s.page, selection: s.selection, before: s.before, after: s.after, stream: { id } },
      { id },
    );
    if (card()?.phase !== "stopped") patch(id, { phase: "done", text: result.text, result });
    void refreshLanguageModels();
  } catch (e) {
    if (card()?.phase === "stopped") return;
    const err = e instanceof BusError ? e.error : null;
    patch(id, { phase: "failed", error: { kind: err?.code === "failed" ? err.kind : (err?.code ?? ""), message: String((e as Error).message ?? e) } });
  }
}
