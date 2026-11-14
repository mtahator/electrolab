// The small bar shown over selected text (P1.M3.UI.9, Scholar parity): highlight colours, other
// marks, comment, copy and copy with citation. One click makes the mark; it is stored at once.
// ("Explain" joins this bar with the AI features, Phase 1 M5.)

import { t } from "@electropaper/kit";
import { For, type JSX } from "solid-js";

import { COLORS } from "./state";

export interface SelectionBarProps {
  at: DOMRect;
  onHighlight: (color: string) => void;
  onMark: (kind: "underline" | "strike_out" | "squiggly") => void;
  onComment: () => void;
  onCopy: () => void;
  onCite: () => void;
}

export function SelectionBar(props: SelectionBarProps): JSX.Element {
  const W = 400;
  const left = Math.max(8, Math.min(window.innerWidth - W - 8, props.at.left + props.at.width / 2 - W / 2));
  const above = props.at.top - 44;
  const top = above < 60 ? props.at.bottom + 8 : above;
  // Keep the selection: pressing a button must not clear it before the click runs.
  const keep = (e: MouseEvent): void => e.preventDefault();
  return (
    <div class="selection-bar" role="toolbar" aria-label={t("annot.selection_bar")} style={{ left: `${left}px`, top: `${top}px` }} onMouseDown={keep}>
      <For each={COLORS}>
        {(c) => <button type="button" class="swatch" style={{ background: c }} title={t("annot.highlight_color")} aria-label={`${t("annot.kind.highlight")} ${c}`} onClick={() => props.onHighlight(c)} />}
      </For>
      <span class="sep" />
      <button type="button" class="bar-btn underline" title={t("annot.kind.underline")} onClick={() => props.onMark("underline")}>
        U
      </button>
      <button type="button" class="bar-btn strike" title={t("annot.kind.strike_out")} onClick={() => props.onMark("strike_out")}>
        S
      </button>
      <button type="button" class="bar-btn squiggly" title={t("annot.kind.squiggly")} onClick={() => props.onMark("squiggly")}>
        ~
      </button>
      <span class="sep" />
      <button type="button" class="bar-btn" onClick={props.onComment}>
        {t("annot.comment")}
      </button>
      <button type="button" class="bar-btn" onClick={props.onCopy}>
        {t("annot.copy")}
      </button>
      <button type="button" class="bar-btn" title={t("annot.cite_hint")} onClick={props.onCite}>
        {t("annot.cite")}
      </button>
    </div>
  );
}
