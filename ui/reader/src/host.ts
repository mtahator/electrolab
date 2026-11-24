// The one place that talks to the Tauri host. Everything else uses typed `bus` calls.

import { type CallOptions, type CommandName, type Commands, call } from "@electropaper/bindings";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

/** Call a bus command. Rejects with `BusError`. */
export function bus<K extends CommandName>(
  command: K,
  input: Commands[K]["input"],
  options?: CallOptions,
): Promise<Commands[K]["output"]> {
  return call(invoke, command, input, options);
}

/** An event published on the bus hub. */
export interface BusEvent {
  kind: string;
  data: unknown;
  at_ms: number;
}

/** Receive hub events whose kind starts with `prefix`. Returns an unsubscribe function. */
export async function onBusEvent(prefix: string, f: (e: BusEvent) => void): Promise<() => void> {
  return listen<BusEvent>("bus-event", (e) => {
    if (e.payload.kind.startsWith(prefix)) f(e.payload);
  });
}

const base = (): string => convertFileSrc("", "ep");

/** URL of a stored PDF, served by the host's `ep` protocol with range requests. */
export function blobUrl(hash: string): string {
  // `convertFileSrc("", "ep")` is `http://ep.localhost/` on Windows and `ep://localhost/` elsewhere.
  return `${base()}blob/${hash}`;
}

/** URL of a rendered page image from the render cache. */
export function renderUrl(hash: string): string {
  return `${base()}render/${hash}`;
}

/** Ask the user for PDF files. */
export async function pickPdfs(title: string): Promise<string[]> {
  const picked = await open({ title, multiple: true, directory: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/** Ask the user for a folder. */
export async function pickFolder(title: string): Promise<string | null> {
  const picked = await open({ title, multiple: false, directory: true });
  return typeof picked === "string" ? picked : null;
}

/** Cancel a running call made with this id. */
export function cancelCall(id: string): Promise<boolean> {
  return invoke("bus_cancel", { id });
}

/** Also look for language models in this folder. */
export function addLlmFolder(path: string): Promise<void> {
  return invoke("add_llm_folder", { path });
}

/** Open a web or mail link in the user's browser or mail app (http, https, mailto only). */
export function openExternal(url: string): Promise<void> {
  return invoke("open_external", { url });
}

/** Open a paper's PDF in the user's usual PDF app. */
export function openPaperExternally(hash: string): Promise<void> {
  return invoke("open_paper_externally", { hash });
}

/** Print a paper with the user's usual PDF app. */
export function printPaper(hash: string): Promise<void> {
  return invoke("print_paper", { hash });
}

/** Download a PDF from a web address; returns the temporary file to import. */
export function downloadPdf(url: string): Promise<string> {
  return invoke("download_pdf", { url });
}

/** PDFs given to the app at start-up (taken once). */
export function takePendingFiles(): Promise<string[]> {
  return invoke("take_pending_files");
}

/** PDFs handed over by a second launch (e.g. double-click in Explorer). */
export function onOpenFiles(f: (paths: string[]) => void): Promise<() => void> {
  return listen<string[]>("open-files", (e) => f(e.payload));
}

/** Files dropped on the window. `over` tells whether something is being dragged over it. */
export function onDropFiles(f: (paths: string[]) => void, over: (dragging: boolean) => void): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === "enter" || p.type === "over") over(true);
    else if (p.type === "leave") over(false);
    else if (p.type === "drop") {
      over(false);
      f(p.paths.filter((x) => x.toLowerCase().endsWith(".pdf")));
    }
  });
}
