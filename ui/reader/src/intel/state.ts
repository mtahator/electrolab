// Document intelligence in the UI (P1.M4): background preparation of open papers, page models,
// and the one popover that shows a figure, table, equation or reference in place.

import type { BlobRef, CitationMarker, DocObject, Mention, ModelInfo, Paper, Preparation, StageState } from "@electropaper/bindings";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";

import { bus, onBusEvent } from "../host";

const [preps, setPreps] = createStore<Record<string, Preparation>>({});
export const [models, setModels] = createSignal<ModelInfo[]>([]);
export const [download, setDownload] = createSignal<{ id: string; received: number; total: number; error?: string } | null>(null);

export function preparationOf(hash: string): Preparation | undefined {
  return preps[hash];
}

/** The stage now running for a file, for the status line. */
export function running(hash: string): StageState | undefined {
  return preps[hash]?.stages.find((s) => s.status === "running");
}

/** Start (or resume) background preparation; pages in view first. */
export async function prepare(file: BlobRef, firstPages: number[]): Promise<void> {
  try {
    const p = await bus("document.prepare", { file, first_pages: firstPages });
    setPreps(file.hash, p);
  } catch {
    // Preparation is a convenience; reading never waits for it.
  }
}

export async function refreshModels(): Promise<void> {
  try {
    setModels(await bus("document.models", null));
  } catch {
    setModels([]);
  }
}

/** Whether a paper has scanned pages whose recognised text can be written into its PDF. */
export function canAddTextLayer(hash: string): boolean {
  const p = preps[hash];
  if (!p || p.image_only_pages.length === 0) return false;
  return p.stages.some((s) => s.stage === "ocr" && s.status === "done");
}

/** Write the recognised text of scanned pages into a new version of the paper's PDF, so other
 *  readers can search and select it too. The previous version stays in the library. */
export async function addTextLayer(paper: Paper): Promise<{ paper: Paper; pages: number }> {
  const out = await bus("document.ocr", { file: paper.file });
  const updated = await bus("workspace.set_paper_file", { id: paper.id, file: out.file });
  return { paper: updated, pages: out.pages.length };
}

export const layoutModel = (): ModelInfo | undefined => models().find((m) => m.id === "egret-medium");

/** Download a model the user asked for; progress shows in the status line. */
export async function fetchModel(id: string): Promise<boolean> {
  const m = models().find((x) => x.id === id);
  setDownload({ id, received: 0, total: m?.size ?? 0 });
  try {
    await invoke("fetch_model", { id });
    setDownload(null);
    await refreshModels();
    return true;
  } catch (e) {
    setDownload({ id, received: 0, total: m?.size ?? 0, error: String(e) });
    return false;
  }
}

void listen<{ id: string; received: number; total: number }>("model-progress", (e) => {
  const d = download();
  if (d && !d.error) setDownload({ ...e.payload });
}).catch(() => {});

void onBusEvent("document.prepar", (e) => {
  if (e.kind === "document.preparing") {
    const { file, stage } = e.data as { file: BlobRef; stage: StageState };
    const cur = preps[file.hash];
    const stages = cur ? cur.stages.map((s) => (s.stage === stage.stage ? stage : s)) : [stage];
    setPreps(file.hash, { file, stages, image_only_pages: cur?.image_only_pages ?? [] });
  } else if (e.kind === "document.prepared") {
    const p = e.data as Preparation;
    setPreps(p.file.hash, p);
  }
}).catch(() => {});

// ---- the popover --------------------------------------------------------------------------------

export type Peek =
  | { kind: "object"; at: DOMRect; file: BlobRef; object: DocObject; mention?: Mention }
  | { kind: "citation"; at: DOMRect; file: BlobRef; marker: CitationMarker }
  | { kind: "region"; at: DOMRect; file: BlobRef; page: number; rect: { x0: number; y0: number; x1: number; y1: number } };

export const [peek, setPeek] = createSignal<Peek | null>(null);

/** Mentions and citation markers of a page, fetched once per file and page. */
const pageLinks = new Map<string, Promise<{ mentions: Mention[]; citations: CitationMarker[] }>>();

export function linksOf(file: BlobRef, page: number): Promise<{ mentions: Mention[]; citations: CitationMarker[] }> {
  const key = `${file.hash}:${page}`;
  let p = pageLinks.get(key);
  if (!p) {
    p = Promise.all([
      bus("document.mentions", { file, page }).catch(() => [] as Mention[]),
      bus("document.citation_markers", { file, page }).catch(() => [] as CitationMarker[]),
    ]).then(([mentions, citations]) => ({ mentions, citations }));
    pageLinks.set(key, p);
    // Keep the map small: the most recent pages only.
    if (pageLinks.size > 200) pageLinks.delete(pageLinks.keys().next().value as string);
  }
  return p;
}

/** Objects already found change as preparation runs: forget a file's cached links then. */
export function forgetLinks(hash: string): void {
  for (const k of [...pageLinks.keys()]) if (k.startsWith(`${hash}:`)) pageLinks.delete(k);
}

void onBusEvent("document.prepared", (e) => {
  const p = e.data as Preparation;
  forgetLinks(p.file.hash);
}).catch(() => {});
