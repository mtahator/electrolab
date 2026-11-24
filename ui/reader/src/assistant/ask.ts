// Asking the library (engines/knowledge/CONTRACT.md): the question, what was found so far and the
// summaries it uses. It is kept outside the view, so that going to a paper and back loses nothing.

import { BusError, type DigestInfo, type DigestStatus, type FindResult, type Paper } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus, cancelCall, onBusEvent } from "../host";
import { refreshLanguageModels } from "./explain";

export type AskStep = "understanding" | "related" | "searching" | "nearest" | "done" | "stopped" | "failed";

const [askOpen, setAskOpen] = createSignal(false);
const [question, setQuestion] = createSignal("");
const [step, setStep] = createSignal<AskStep | null>(null);
const [result, setResult] = createSignal<FindResult | null>(null);
const [error, setError] = createSignal<string | null>(null);
/** Every paper of the library, by id. */
const [library, setLibrary] = createSignal<ReadonlyMap<string, Paper>>(new Map());
const [digests, setDigests] = createSignal<ReadonlyMap<string, DigestInfo>>(new Map());
const [status, setStatus] = createSignal<DigestStatus | null>(null);

export { askOpen, digests, error as askError, library, question, result as askResult, setAskOpen, setQuestion, status as digestStatus, step as askStep };

export const asking = (): boolean => ["understanding", "related", "searching"].includes(step() ?? "");
let current = "";

void onBusEvent("knowledge.finding", (e) => {
  const d = e.data as { id: string; step: AskStep; result: FindResult };
  if (d.id !== current || !asking()) return;
  setResult(d.result);
  if (d.step !== "done") setStep(d.step);
});
void onBusEvent("knowledge.digesting", (e) => void setStatus(e.data as DigestStatus));
let timer: number | undefined;
void onBusEvent("knowledge.digested", () => {
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void refreshDigests().catch(() => {}), 600);
});

export async function refreshDigests(): Promise<void> {
  const [infos, papers] = await Promise.all([bus("knowledge.digests", {}), bus("workspace.list_papers", { limit: 10000, offset: 0, sort: "opened" })]);
  setDigests(new Map(infos.map((d) => [d.paper, d])));
  setLibrary(new Map(papers.map((p) => [p.id, p])));
}

/** The view was opened: what is there, and the quick summaries of papers that have none. */
export async function enterAsk(): Promise<void> {
  await Promise.all([refreshDigests(), refreshLanguageModels()]);
  setStatus(await bus("knowledge.digest_status", null));
  if ([...library().keys()].some((id) => !digests().get(id)?.level)) await makeDigests({ full: false, auto: true });
}

/** Make summaries in the background: of these papers, or (`full`) of all. */
export async function makeDigests(i: { full: boolean; papers?: string[]; force?: boolean; auto?: boolean }): Promise<void> {
  setStatus(await bus("knowledge.digest_library", { full: i.full, papers: i.papers ?? null, force: i.force ?? false, auto: i.auto ?? false }));
}

export async function stopDigests(): Promise<void> {
  await bus("knowledge.digest_stop", null);
}

export function stopAsk(): void {
  if (!asking()) return;
  setStep("stopped");
  void cancelCall(current);
}

export async function ask(text: string): Promise<void> {
  const q = text.trim();
  stopAsk();
  if (!q) return;
  const id = crypto.randomUUID();
  current = id;
  setQuestion(text);
  setResult(null);
  setError(null);
  setStep("understanding");
  try {
    const r = await bus("knowledge.find", { question: q, stream: { id } }, { id });
    if (current !== id || step() === "stopped") return;
    setResult(r);
    setStep("done");
  } catch (e) {
    if (current !== id || step() === "stopped") return;
    setError(e instanceof BusError ? e.message : String((e as Error).message ?? e));
    setStep("failed");
  }
}
