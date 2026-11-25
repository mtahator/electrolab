// Explaining (P1.M5.UI.1–3, UND.3–8; design language §17.13): one card at a time, streamed; closing
// it or Stop cancels the answer. A selection is explained by understanding.explain_span; a figure,
// table, equation or paragraph by understanding.explain_region. Pictures are looked at only when
// the reader agreed, once per paper.

import { type BlobRef, BusError, type ExplainDepth, type Explanation, type FigureSeen, type LlmModels, type ObjectExplanation, type Symbol } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus, cancelCall, onBusEvent } from "../host";

/** Where the card belongs. */
interface Anchor {
  paper: string;
  file: BlobRef;
  page: number;
  /** Top of the place on its page, for notes. */
  y: number;
  /** Where it is on screen. */
  at: DOMRect;
}

export interface ExplainSelection extends Anchor {
  selection: string;
  before: string;
  after: string;
}

export type ThingKind = "figure" | "table" | "equation" | "paragraph" | "here";

/** A thing on a page: an object, or whatever is at a place. */
export interface ExplainThing extends Anchor {
  title: string;
  kind: ThingKind;
  object?: string;
  rect?: { x0: number; y0: number; x1: number; y1: number };
}

export type Phase = "looking" | "starting" | "draft" | "checking" | "done" | "stopped" | "failed";

export interface ExplainCardState extends Anchor {
  id: string;
  title: string;
  kind: ThingKind | "selection";
  subject: ExplainSelection | ExplainThing;
  depth: ExplainDepth;
  phase: Phase;
  text: string;
  symbols: Symbol[];
  /** The answer about a selection. */
  result?: Explanation;
  /** The answer about a thing. */
  about?: ObjectExplanation;
  /** What was seen in the picture. */
  seen?: { text: string; model: string };
  /** The picture is being looked at. */
  looking?: { id: string; text: string };
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

/** Whether pictures can be looked at on this machine now. */
export const canLook = (): boolean => {
  const m = models();
  return !!m && m.runtime.backend !== "none" && !!m.vision;
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

/** The order in which models are tried (empty: automatic), and whether one is kept ready. */
export async function chooseModels(i: { order?: string[]; keep_ready?: boolean }): Promise<void> {
  setModels(await bus("inference.choose", i));
}

const answering = (p: Phase): boolean => p === "starting" || p === "draft" || p === "checking";
export const explainRunning = (): boolean => answering(card()?.phase ?? "done") || !!card()?.looking;
const patch = (id: string, change: Partial<ExplainCardState>): void => {
  setCard((c) => (c && c.id === id ? { ...c, ...change } : c));
};
const problem = (e: unknown): { kind: string; message: string } => {
  const err = e instanceof BusError ? e.error : null;
  return { kind: err?.code === "failed" ? err.kind : (err?.code ?? ""), message: String((e as Error).message ?? e) };
};

void onBusEvent("understanding.explaining", (e) => {
  const d = e.data as { id: string; phase: string; text: string };
  const c = card();
  if (!c || c.id !== d.id || !answering(c.phase)) return;
  patch(d.id, { ...(d.text ? { text: d.text } : {}), phase: d.phase === "draft" ? "draft" : "checking" });
});

void onBusEvent("understanding.symbols", (e) => {
  const d = e.data as { id: string; symbols: Symbol[] };
  patch(d.id, { symbols: d.symbols });
});

void onBusEvent("understanding.looking", (e) => {
  const d = e.data as { id: string; done: number; total: number; text: string };
  const c = card();
  if (c?.looking?.id === d.id && d.text) patch(c.id, { looking: { id: d.id, text: d.text } });
  setAllLooked((a) => (a && a.id === d.id ? { ...a, done: d.done, total: d.total } : a));
});

export function stopExplain(): void {
  const c = card();
  if (!c) return;
  if (c.looking) {
    void cancelCall(c.looking.id);
    patch(c.id, { looking: undefined, ...(c.phase === "looking" ? { phase: "stopped" as const } : {}) });
  }
  if (!answering(c.phase)) return;
  patch(c.id, { phase: "stopped" });
  void cancelCall(c.id);
}

export function closeExplain(): void {
  stopExplain();
  setCard(null);
}

export const isSelection = (s: ExplainSelection | ExplainThing): s is ExplainSelection => "selection" in s;

/** Open the card for a subject. */
function open(s: ExplainSelection | ExplainThing, phase: Phase): string {
  closeExplain();
  const id = crypto.randomUUID();
  const { paper, file, page, y, at } = s;
  const [title, kind] = isSelection(s) ? ([s.selection, "selection"] as const) : ([s.title, s.kind] as const);
  setCard({ id, paper, file, page, y, at, title, kind, subject: s, depth: "normal", phase, text: "", symbols: [] });
  return id;
}

/** Ask for the answer of the card that is open. */
async function answer(id: string): Promise<void> {
  const c = card();
  if (!c || c.id !== id) return;
  const s = c.subject;
  const here = (): boolean => card()?.id === id && card()?.phase !== "stopped";
  try {
    if (isSelection(s)) {
      const result = await bus(
        "understanding.explain_span",
        { file: s.file, page: s.page, selection: s.selection, before: s.before, after: s.after, depth: c.depth, stream: { id } },
        { id },
      );
      if (here()) patch(id, { phase: "done", text: result.text, result });
    } else {
      const where = s.object ? { object: s.object } : s.rect ? { rect: { page: s.page, ...s.rect } } : {};
      const about = await bus(
        "understanding.explain_region",
        { file: s.file, paper: s.paper, page: s.page, depth: c.depth, stream: { id }, ...where },
        { id },
      );
      if (here()) patch(id, { phase: "done", text: about.text, about, symbols: about.symbols ?? [], ...(about.seen ? { seen: seenOf(about.seen) } : {}) });
    }
    void refreshLanguageModels();
  } catch (e) {
    if (here()) patch(id, { phase: "failed", error: problem(e) });
  }
}

const seenOf = (f: FigureSeen): { text: string; model: string } => ({ text: f.text, model: f.model });

export async function explain(s: ExplainSelection | ExplainThing): Promise<void> {
  await answer(open(s, "starting"));
}

/** Answer again in the same card, simpler or deeper (UND.8); the old answer stays until the new
 *  one begins. */
export async function explainAgain(depth: ExplainDepth): Promise<void> {
  const c = card();
  if (!c || explainRunning()) return;
  const id = crypto.randomUUID();
  setCard({ ...c, id, depth, phase: "starting", error: undefined });
  await answer(id);
}

// ---- pictures (UI.3, UND.5) ----------------------------------------------------------------------

/** Papers whose pictures may be looked at (asked once per paper). */
const agreed = new Set<string>();

type Agreed = "this" | "all" | null;
export interface LookQuestion {
  thing: ExplainThing;
  /** Figures of the paper. */
  total: number;
  answer: (a: Agreed) => void;
}

const [question, setQuestion] = createSignal<LookQuestion | null>(null);
const [allLooked, setAllLooked] = createSignal<{ id: string; done: number; total: number } | null>(null);
/** The consent dialog; all figures of a paper being looked at (status line). */
export { question as lookQuestion, allLooked as lookingAtAll };

export function stopLookingAtAll(): void {
  const a = allLooked();
  if (a) void cancelCall(a.id);
  setAllLooked(null);
}

/** Ask once per paper whether its pictures may be looked at. */
async function consent(thing: ExplainThing): Promise<Agreed> {
  if (agreed.has(thing.paper)) return "this";
  const seen = await bus("understanding.figures_seen", { paper: thing.paper, file: thing.file }).catch(() => null);
  if (seen && seen.figures.length > 0) {
    agreed.add(thing.paper);
    return "this";
  }
  const said = await new Promise<Agreed>((done) => setQuestion({ thing, total: seen?.total ?? 1, answer: done }));
  setQuestion(null);
  if (said) agreed.add(thing.paper);
  return said;
}

/** The other figures, in the background; the status line counts them. */
async function lookAtAll(thing: ExplainThing): Promise<void> {
  if (allLooked()) return;
  const id = crypto.randomUUID();
  setAllLooked({ id, done: 0, total: 0 });
  await bus("understanding.look_at_figures", { paper: thing.paper, file: thing.file, consent: true, stream: { id } }, { id }).catch(() => null);
  setAllLooked((a) => (a?.id === id ? null : a));
}

/** Look at the picture of a figure: in the card that shows it, or in a new one that then explains
 *  it with what was seen. */
export async function lookAt(thing: ExplainThing): Promise<void> {
  if (!thing.object) return;
  const said = await consent(thing);
  if (!said) return;
  const c = card();
  const shown = !!c && !isSelection(c.subject) && c.subject.object === thing.object && c.file.hash === thing.file.hash;
  const id = shown && c ? c.id : open(thing, "looking");
  const look = crypto.randomUUID();
  patch(id, { looking: { id: look, text: "" }, seen: undefined, error: undefined });
  const here = (): boolean => card()?.looking?.id === look;
  try {
    const r = await bus(
      "understanding.look_at_figures",
      { paper: thing.paper, file: thing.file, consent: true, objects: [{ page: thing.page, id: thing.object }], stream: { id: look } },
      { id: look },
    );
    if (said === "all") void lookAtAll(thing);
    if (!here()) return;
    const f = r.figures.find((x) => x.object.id === thing.object && x.object.page === thing.page);
    patch(id, { looking: undefined, ...(f ? { seen: seenOf(f) } : {}), ...(shown ? {} : { phase: canExplain() ? "starting" : "done" }) });
    if (!shown && canExplain()) await answer(id);
  } catch (e) {
    if (here()) patch(id, { looking: undefined, error: problem(e), ...(shown ? {} : { phase: "failed" }) });
  }
}
