// Paper notes in the UI (P1.MD.NOTE.1–3, guideline §10.2): the notebook of each open paper,
// saved shortly after typing stops, and "Add to notes" with location chips.

import { BusError, type PaperNotes } from "@electropaper/bindings";
import { showToast, t } from "@electropaper/kit";
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";

import { bus, onBusEvent } from "../host";
import { setRightPanel } from "../shell/frame";

const SAVE_AFTER_MS = 600;

interface Entry {
  text: string;
  version: number;
  /** Text typed but not yet stored. */
  dirty: boolean;
  loaded: boolean;
}

const [notes, setNotes] = createStore<Record<string, Entry>>({});
const timers = new Map<string, number>();
/** A quote waiting to be inserted by the open editor (it knows the cursor). */
export const [incoming, setIncoming] = createSignal<{ paper: string; text: string; seq: number } | null>(null);
let report: (e: unknown) => void = (e) => console.error(e);
let seq = 0;

export function onNotesError(f: (e: unknown) => void): void {
  report = f;
}

export function notesOf(paper: string): Entry | undefined {
  return notes[paper];
}

export async function loadNotes(paper: string): Promise<void> {
  if (notes[paper]?.dirty) return;
  try {
    const n = await bus("workspace.get_notes", { paper });
    // Typing or a quote that arrived while loading wins (it is saved on top of this version).
    const cur = notes[paper];
    if (cur?.dirty) return void setNotes(paper, { version: n.version, loaded: true });
    setNotes(paper, { text: n.text, version: n.version, dirty: false, loaded: true });
  } catch (e) {
    report(e);
  }
}

/** The editor reports every change; the text is stored shortly after typing stops. */
export function edit(paper: string, text: string): void {
  setNotes(paper, (e) => ({ ...(e ?? { version: 0, loaded: true }), text, dirty: true }));
  window.clearTimeout(timers.get(paper));
  timers.set(paper, window.setTimeout(() => void flush(paper), SAVE_AFTER_MS));
}

/** Store pending text now (also called when the window hides or the panel closes). */
export async function flush(paper: string): Promise<void> {
  window.clearTimeout(timers.get(paper));
  const e = notes[paper];
  if (!e?.dirty) return;
  const text = e.text;
  try {
    const n: PaperNotes = await bus("workspace.save_notes", { paper, text, base_version: e.version });
    setNotes(paper, (cur) => ({ ...cur, version: n.version, dirty: cur.text !== text }));
  } catch (err) {
    const ce = err instanceof BusError ? err.error : null;
    if (ce?.code === "failed" && ce.kind === "conflict") {
      // Another window saved in between: keep what was typed here, on top of its version
      // (that version stays in the history).
      const cur = await bus("workspace.get_notes", { paper });
      const n = await bus("workspace.save_notes", { paper, text, base_version: cur.version });
      setNotes(paper, (c) => ({ ...c, version: n.version, dirty: c.text !== text }));
      showToast(t("notes.conflict"));
    } else {
      report(err);
    }
  }
}

export async function flushAll(): Promise<void> {
  await Promise.all(Object.keys(notes).map((p) => flush(p)));
}

/** A location chip: `[p. 4](loc:4/120)`, optionally with the annotation id. */
export function chip(page: number, y: number, annotation?: string): string {
  return `[p. ${page}](loc:${page}/${Math.round(y)}${annotation ? `/${annotation}` : ""})`;
}

/** Parse a chip target. */
export function parseLoc(href: string): { page: number; y: number; annotation?: string } | null {
  const m = /^loc:(\d+)\/(\d+)(?:\/([\w.:-]+))?$/.exec(href);
  if (!m) return null;
  return { page: Number(m[1]), y: Number(m[2]), annotation: m[3] };
}

/** "Add to notes": a quote with a chip back to its place; opens the Notes panel. */
export async function quoteToNotes(paper: string, q: { text: string; page: number; y: number; annotation?: string }): Promise<void> {
  if (!notes[paper]?.loaded) await loadNotes(paper);
  const quote = q.text.trim().replace(/\s*\n\s*/g, " ");
  const block = `${quote ? `> ${quote}\n` : ""}${quote ? "> " : ""}${chip(q.page, q.y, q.annotation)}\n\n`;
  setRightPanel("notes");
  seq += 1;
  setIncoming({ paper, text: block, seq });
}

/** Annotations quoted in the notes (their cards show a note icon). */
export function quotedAnnotations(paper: string): Set<string> {
  const text = notes[paper]?.text ?? "";
  const out = new Set<string>();
  for (const m of text.matchAll(/\(loc:\d+\/\d+\/([\w.:-]+)\)/g)) if (m[1]) out.add(m[1]);
  return out;
}

window.addEventListener("pagehide", () => void flushAll());
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") void flushAll();
});

void onBusEvent("workspace.notes_changed", (e) => {
  const d = e.data as { paper?: string; version?: number };
  if (!d.paper || d.version === undefined) return;
  const cur = notes[d.paper];
  if (cur && !cur.dirty && d.version > cur.version) void loadNotes(d.paper);
}).catch(() => {});
