// The one place that talks to the Tauri host. Everything else uses typed `bus` calls.

import { type CallOptions, type CommandName, type Commands, call } from "@electropaper/bindings";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

/** URL of a stored file, served by the host's `ep` protocol with range requests. */
export function blobUrl(hash: string): string {
  // `convertFileSrc("", "ep")` is `http://ep.localhost/` on Windows and `ep://localhost/` elsewhere.
  return `${convertFileSrc("", "ep")}blob/${hash}`;
}

/** Ask the user for PDF files. */
export async function pickPdfs(title: string): Promise<string[]> {
  const picked = await open({ title, multiple: true, directory: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}
