// What things on a page are called, which one is under a point, and copying them (used by the
// popover, the right-click menu and the Explain card).

import type { BlobRef, DocObject } from "@electropaper/bindings";
import { showToast, t } from "@electropaper/kit";

import type { ThingKind } from "../assistant/explain";
import { bus, renderUrl } from "../host";
import { pageColors } from "../viewer/Viewer";

type Box = { x0: number; y0: number; x1: number; y1: number };

/** "Figure 3" and "Table 2" are printed labels; sections and equations need their word. */
export function nameOf(o: DocObject): string {
  const word = { figure: t("intel.figure"), caption: t("intel.figure"), table: t("intel.table"), equation: t("intel.equation"), heading: t("intel.section") }[o.kind as string] ?? t("intel.region");
  if (!o.label) return word;
  return o.kind === "heading" || o.kind === "equation" ? `${word} ${o.label}` : o.label;
}

/** What an object is for an explanation. */
export function kindOf(o: DocObject): ThingKind {
  if (o.kind === "figure" || o.kind === "table" || o.kind === "equation") return o.kind;
  return o.kind === "text" || o.kind === "list_item" ? "paragraph" : "here";
}

const has = (r: Box | null | undefined, x: number, y: number): boolean => !!r && x >= r.x0 - 2 && x <= r.x1 + 2 && y >= r.y0 - 2 && y <= r.y1 + 2;
const area = (r: Box): number => (r.x1 - r.x0) * (r.y1 - r.y0);
const MAIN = ["figure", "table", "equation"];

/** The thing under a point: the smallest figure, table or equation there, else the smallest
 *  paragraph. A caption stands for its figure or table. */
export function objectAt(objects: DocObject[], x: number, y: number): DocObject | null {
  const captions = objects.filter((o) => MAIN.includes(o.kind) && has(o.caption_rect as Box | null, x, y));
  const printed = objects.find((o) => o.kind === "caption" && has(o.rect, x, y));
  const ofCaption = printed?.label ? objects.filter((o) => MAIN.includes(o.kind) && o.label === printed.label) : [];
  const under = objects.filter((o) => has(o.rect, x, y));
  const least = (os: DocObject[]): DocObject | null => [...os].sort((a, b) => area(a.rect) - area(b.rect))[0] ?? null;
  return least(under.filter((o) => MAIN.includes(o.kind))) ?? least(captions) ?? least(ofCaption) ?? least(under.filter((o) => kindOf(o) === "paragraph"));
}

/** Copy a picture kept by the app. */
export async function copyImage(hash: string): Promise<void> {
  const blob = await (await fetch(renderUrl(hash))).blob();
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
  showToast(t("intel.copied_image"));
}

/** Copy a part of a page as a picture. */
export async function copyPart(file: BlobRef, rect: DocObject["rect"]): Promise<void> {
  const colors = pageColors();
  const hit = await bus("document.region", { file, page: rect.page, rect, exact: true, width_px: 1200, ...(colors ? { colors } : {}) });
  await copyImage(hit.image.hash);
}
