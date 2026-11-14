// Margin view (P1.MD.NOTE.5, guideline §10.1): sticky notes and commented marks of one page as
// small cards in a column beside it, aligned to their spot, stacked without overlap, with a thin
// leader line to the page edge. Off by default; switched per paper.

import type { Annotation } from "@electropaper/bindings";
import { Icon, t } from "@electropaper/kit";
import { For, type JSX, createMemo } from "solid-js";

import { KIND_ICON } from "./Editor";
import { annotationsOf, isMarkup, marksHidden, picked, repliesOf, setPicked } from "./state";

/** Width of the margin column (CSS pixels), including its gap to the page. */
export const MARGIN_W = 264;
const GAP = 16;
const CARD_H = 76;
const SPACE = 8;

export interface Placed {
  a: Annotation;
  /** Where the note sits on the page (CSS pixels from the page top). */
  anchor: number;
  /** Where the card is drawn. */
  top: number;
}

/** Stack cards top to bottom: each at its anchor, or just below the card before it. */
export function stack(items: { a: Annotation; anchor: number }[], height = CARD_H, space = SPACE): Placed[] {
  const out: Placed[] = [];
  let next = 0;
  for (const it of [...items].sort((x, y) => x.anchor - y.anchor)) {
    const top = Math.max(it.anchor - 8, next);
    out.push({ ...it, top });
    next = top + height + space;
  }
  return out;
}

export function MarginNotes(props: { paper: string; page: number; scale: number; pageWidth: number }): JSX.Element {
  const placed = createMemo(() => {
    if (marksHidden()) return [];
    const items = (annotationsOf(props.paper)?.annotations ?? [])
      .filter((a) => a.page === props.page && !a.parent && (a.kind === "note" || (isMarkup(a.kind) && !!a.body)))
      .map((a) => ({ a, anchor: a.rect.y0 * props.scale }));
    return stack(items);
  });
  return (
    <div class="margin-notes" style={{ left: `${props.pageWidth + GAP}px`, width: `${MARGIN_W - GAP - 8}px` }} aria-label={t("notes.margin_view")}>
      <svg class="margin-leaders" width={GAP} height="100%" style={{ left: `${-GAP}px` }} aria-hidden="true">
        <For each={placed()}>{(p) => <path d={`M0 ${p.anchor + 6} C ${GAP / 2} ${p.anchor + 6}, ${GAP / 2} ${p.top + 14}, ${GAP} ${p.top + 14}`} />}</For>
      </svg>
      <For each={placed()}>
        {(p) => {
          const replies = (): number => repliesOf(props.paper, p.a.id).length;
          return (
            <button
              type="button"
              class="margin-card"
              classList={{ resolved: !!p.a.resolved, picked: picked()?.id === p.a.id }}
              style={{ top: `${p.top}px`, "border-inline-start-color": p.a.color }}
              onClick={() => setPicked({ paper: props.paper, id: p.a.id, edit: true })}
            >
              <span class="margin-card-head">
                <Icon name={KIND_ICON[p.a.kind]} size={14} />
                <span>{p.a.author ?? ""}</span>
                <span class="ep-spacer" />
                {replies() > 0 ? <span>{t("annot.replies", { n: replies() })}</span> : null}
              </span>
              <span class="margin-card-body">{p.a.body || p.a.text || ""}</span>
            </button>
          );
        }}
      </For>
    </div>
  );
}
