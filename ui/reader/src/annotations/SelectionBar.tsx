// The small bar over selected text (P1.M3.UI.9, Scholar parity; guideline §9.1): highlight
// colours first, then other marks, comment, add to notes, copy and cite — icons with tooltips.
// One click makes the mark; it is stored at once. Explain (AI) comes first after the colours.

import { IconButton, t } from "@electropaper/kit";
import { For, type JSX } from "solid-js";

import { COLORS, colorNames } from "./state";

export interface SelectionBarProps {
  at: DOMRect;
  onHighlight: (color: string) => void;
  onMark: (kind: "underline" | "strike_out" | "squiggly") => void;
  onComment: () => void;
  onCopy: () => void;
  onCite: () => void;
  onAddToNotes: () => void;
  onExplain: () => void;
}

export function SelectionBar(props: SelectionBarProps): JSX.Element {
  const W = 460;
  const left = Math.max(8, Math.min(window.innerWidth - W - 8, props.at.left + props.at.width / 2 - W / 2));
  const above = props.at.top - 48;
  const top = above < 60 ? props.at.bottom + 8 : above;
  // Keep the selection: pressing a button must not clear it before the click runs.
  const keep = (e: MouseEvent): void => e.preventDefault();
  return (
    <div class="selection-bar ep-card" role="toolbar" aria-label={t("annot.selection_bar")} style={{ left: `${left}px`, top: `${top}px` }} onMouseDown={keep}>
      <For each={COLORS}>
        {(c) => (
          <button
            type="button"
            class="ep-swatch"
            style={{ background: c }}
            data-tip={`${t("annot.kind.highlight")}: ${colorNames[c] ?? c}`}
            aria-label={`${t("annot.kind.highlight")}: ${colorNames[c] ?? c}`}
            onClick={() => props.onHighlight(c)}
          />
        )}
      </For>
      <span class="bar-sep" />
      <IconButton icon="sparkles" size="sm" label={t("explain.action")} onClick={props.onExplain} />
      <span class="bar-sep" />
      <IconButton icon="underline" size="sm" label={t("annot.kind.underline")} onClick={() => props.onMark("underline")} />
      <IconButton icon="strikethrough" size="sm" label={t("annot.kind.strike_out")} onClick={() => props.onMark("strike_out")} />
      <IconButton icon="spline" size="sm" label={t("annot.kind.squiggly")} onClick={() => props.onMark("squiggly")} />
      <span class="bar-sep" />
      <IconButton icon="message-circle-plus" size="sm" label={t("annot.comment")} onClick={props.onComment} />
      <IconButton icon="notebook-pen" size="sm" label={t("notes.add_to_notes")} onClick={props.onAddToNotes} />
      <IconButton icon="copy" size="sm" label={t("annot.copy")} shortcut="Ctrl+C" onClick={props.onCopy} />
      <IconButton icon="quote" size="sm" label={t("annot.cite_hint")} shortcut="Ctrl+Shift+C" onClick={props.onCite} />
    </div>
  );
}
