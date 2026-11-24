// Typed access to the command bus. `types.ts` and `commands.ts` are generated; this file is not.
//
// The bindings do not depend on Tauri: a host passes its own `Invoke` (Tauri's `invoke` in the
// Reader, a fake in tests).

import type { CommandName, Commands } from "./commands";
import type { CommandError } from "./types";

export type * from "./types";
export type { CommandName, Commands } from "./commands";

/** Sends one call to the host; rejects with a `CommandError` value. */
export type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;

/** Options of one call. */
export interface CallOptions {
  /** Give up after this many milliseconds. */
  deadlineMs?: number;
  /** Policy rules the user approved for this call (after a `needs_approval` error). */
  approved?: string[];
  /** Id of the call, so it can be cancelled (a UUID). */
  id?: string;
}

/** A failed call, carrying the bus error. */
export class BusError extends Error {
  readonly error: CommandError;

  constructor(error: CommandError) {
    super(describe(error));
    this.name = "BusError";
    this.error = error;
  }
}

/** One line for people. */
export function describe(e: CommandError): string {
  switch (e.code) {
    case "unknown_command":
      return `Unknown command ${e.command}`;
    case "invalid_input":
    case "transport":
      return e.message;
    case "denied":
      return `Not allowed: ${e.reason}`;
    case "needs_approval":
      return `Needs your approval: ${e.reason}`;
    case "cancelled":
      return "Cancelled";
    case "deadline_exceeded":
      return `Took longer than ${e.deadline_ms} ms`;
    case "failed":
      return e.message;
  }
}

function isCommandError(v: unknown): v is CommandError {
  return typeof v === "object" && v !== null && typeof (v as { code?: unknown }).code === "string";
}

/** Call a command with typed input and output. */
export async function call<K extends CommandName>(
  invoke: Invoke,
  command: K,
  input: Commands[K]["input"],
  options: CallOptions = {},
): Promise<Commands[K]["output"]> {
  try {
    return (await invoke("bus_call", {
      command,
      input,
      deadlineMs: options.deadlineMs ?? null,
      approved: options.approved ?? [],
      id: options.id ?? null,
    })) as Commands[K]["output"];
  } catch (e) {
    throw new BusError(isCommandError(e) ? e : { code: "transport", message: String(e) });
  }
}
