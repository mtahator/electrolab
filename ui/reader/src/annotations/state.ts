// Annotation state shared by the viewer, toolbar, editor and sidebar (P1.M3.MOD.3, UI.1–7).
//
// The Document engine stores every change at once (autosave, ADR-006); this module keeps the
// latest `AnnotationSet` of each open paper, the current tool and its options, and the selection.

import type { Annotation, AnnotationKind, AnnotationOp, AnnotationSet, Category, Paper } from "@electropaper/bindings";
import { createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";

import { bus, onBusEvent } from "../host";
import { newId } from "./geometry";

/** What a click or drag on the page does. */
export type Tool = "select" | AnnotationKind | "eraser";

/** Highlight colours offered first (Scholar parity: a few colours, one click). */
export const COLORS = ["#ffd400", "#7bd88f", "#74c0fc", "#ff8787", "#d0a2f7"] as const;
/** Colours for drawings, shapes and text. */
export const INK_COLORS = ["#e03131", "#1971c2", "#2f9e44", "#f08c00", "#212529"] as const;
export const CATEGORIES: Category[] = ["definition", "method", "result", "question", "to_check"];
export const MARKUP: AnnotationKind[] = ["highlight", "underline", "strike_out", "squiggly"];

export const isMarkup = (k: string): k is "highlight" | "underline" | "strike_out" | "squiggly" => (MARKUP as string[]).includes(k);

export const [tool, setTool] = createSignal<Tool>("select");
export const [markColor, setMarkColor] = createSignal<string>(COLORS[0]);
export const [inkColor, setInkColor] = createSignal<string>(INK_COLORS[0]);
export const [category, setCategory] = createSignal<Category | null>(null);
export const [strokeWidth, setStrokeWidth] = createSignal(1.5);
export const [fontSize, setFontSize] = createSignal(12);

/** The annotation picked on a page, and whether its editor card is open. */
export interface Picked {
  paper: string;
  id: string;
  edit: boolean;
  /** Put the cursor in the comment box when the card opens. */
  focus?: boolean;
}
export const [picked, setPicked] = createSignal<Picked | null>(null);

/** A new annotation waiting for its text (a note, free text, or a comment on a selection):
 *  it is added only when the text is saved, so cancelling leaves nothing behind. */
export const [pending, setPending] = createSignal<{ paper: string; annotation: Annotation } | null>(null);

/** Last time a change reached the store (for the "Saved" indicator). */
export const [savedAt, setSavedAt] = createSignal<number | null>(null);

const [sets, setSets] = createStore<Record<string, AnnotationSet>>({});
const files = new Map<string, Paper["file"]>();
let report: (e: unknown) => void = (e) => console.error(e);

/** Where errors go (the shell's banner). */
export function onAnnotationError(f: (e: unknown) => void): void {
  report = f;
}

export function annotationsOf(paper: string): AnnotationSet | undefined {
  return sets[paper];
}

export function find(paper: string, id: string): Annotation | undefined {
  return sets[paper]?.annotations.find((a) => a.id === id);
}

export function repliesOf(paper: string, id: string): Annotation[] {
  return (sets[paper]?.annotations ?? []).filter((a) => a.parent === id);
}

function put(set: AnnotationSet): void {
  setSets(set.paper, reconcile(set, { key: "id", merge: true }));
}

/** Load a paper's annotations (imports those already in its PDF the first time). */
export async function load(paper: Paper): Promise<void> {
  files.set(paper.id, paper.file);
  try {
    put(await bus("document.annotations", { paper: paper.id, file: paper.file }));
  } catch (e) {
    report(e);
  }
}

/** Apply changes as one undoable edit; stored at once. */
export async function apply(paper: string, ops: AnnotationOp[]): Promise<boolean> {
  if (ops.length === 0) return true;
  try {
    put(await bus("document.annotations_apply", { paper, ops }));
    setSavedAt(Date.now());
    return true;
  } catch (e) {
    report(e);
    return false;
  }
}

export const add = (paper: string, annotations: Annotation[]): Promise<boolean> => apply(paper, annotations.map((annotation) => ({ op: "add", annotation })));
export const change = (paper: string, annotation: Annotation): Promise<boolean> => apply(paper, [{ op: "change", annotation }]);
export const remove = (paper: string, id: string): Promise<boolean> => {
  const p = picked();
  if (p && p.paper === paper && p.id === id) setPicked(null);
  return apply(paper, [{ op: "remove", id }]);
};

export async function undo(paper: string): Promise<void> {
  try {
    put(await bus("document.annotations_undo", { paper }));
    setSavedAt(Date.now());
  } catch (e) {
    report(e);
  }
}

export async function redo(paper: string): Promise<void> {
  try {
    put(await bus("document.annotations_redo", { paper }));
    setSavedAt(Date.now());
  } catch (e) {
    report(e);
  }
}

/** "Save into PDF" (Ctrl+S, P1.M3.PDF.3): write the annotations into a new version of the
 *  paper's managed copy and point the paper at it. The previous version stays in the library. */
export async function saveIntoPdf(paperId: string): Promise<Paper | null> {
  try {
    const paper = await bus("workspace.get_paper", { id: paperId });
    const saved = await bus("document.annotate", { paper: paperId, file: paper.file });
    const updated = await bus("workspace.set_paper_file", { id: paperId, file: saved.file });
    files.set(paperId, updated.file);
    await refresh(paperId);
    return updated;
  } catch (e) {
    report(e);
    return null;
  }
}

async function refresh(paper: string): Promise<void> {
  const file = files.get(paper);
  if (!file) return;
  try {
    put(await bus("document.annotations", { paper, file }));
  } catch {
    // The next edit brings the state back.
  }
}

/** Keep every view of a paper current when another pane changes it. */
void onBusEvent("document.annotations_changed", (e) => {
  const paper = (e.data as { paper?: string }).paper;
  if (paper) void refresh(paper);
}).catch(() => {});

/** The tool's colour: highlight colours for text marks and notes, ink colours otherwise. */
export function colorFor(kind: AnnotationKind): string {
  return isMarkup(kind) || kind === "note" ? markColor() : inkColor();
}

/** A new annotation with the current options. */
export function draft(kind: AnnotationKind, page: number, fields: Partial<Annotation>): Annotation {
  const now = Date.now();
  return {
    id: newId(),
    page,
    kind,
    rect: { page, x0: 0, y0: 0, x1: 0, y1: 0 },
    color: colorFor(kind),
    opacity: 1,
    width: strokeWidth(),
    font_size: kind === "free_text" ? fontSize() : null,
    category: category(),
    body: "",
    created_ms: now,
    modified_ms: now,
    ...fields,
  };
}

/** Whether keyboard focus is in a text field (editing shortcuts then belong to the field). */
export function typing(): boolean {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
}
