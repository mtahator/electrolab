// The annotation tool bar under the top bar (P1.M3.UI.2–6): tools, colour, category, line width,
// undo and redo, the saved indicator and "Save into PDF".

import type { Category } from "@electropaper/bindings";
import { type MessageKey, t } from "@electropaper/kit";
import { For, type JSX, Show } from "solid-js";

import { categoryLabel } from "./Editor";
import {
  CATEGORIES,
  COLORS,
  INK_COLORS,
  type Tool,
  annotationsOf,
  category,
  inkColor,
  isMarkup,
  markColor,
  redo,
  savedAt,
  setCategory,
  setInkColor,
  setMarkColor,
  setStrokeWidth,
  setTool,
  strokeWidth,
  tool,
  undo,
} from "./state";

const TOOLS: { tool: Tool; key: string }[] = [
  { tool: "select", key: "V" },
  { tool: "highlight", key: "H" },
  { tool: "underline", key: "" },
  { tool: "strike_out", key: "" },
  { tool: "note", key: "N" },
  { tool: "free_text", key: "T" },
  { tool: "ink", key: "D" },
  { tool: "rectangle", key: "" },
  { tool: "ellipse", key: "" },
  { tool: "line", key: "" },
  { tool: "arrow", key: "" },
  { tool: "eraser", key: "E" },
];

export const toolLabel = (tl: Tool): string => t(tl === "select" ? "annot.tool.select" : tl === "eraser" ? "annot.tool.eraser" : (`annot.kind.${tl}` as MessageKey));

export function Toolbar(props: { paper: string; onSave: () => void; saving: boolean }): JSX.Element {
  const set = (): ReturnType<typeof annotationsOf> => annotationsOf(props.paper);
  const usesMarkColors = (): boolean => isMarkup(tool()) || tool() === "note" || tool() === "select";
  return (
    <div class="annot-bar" role="toolbar" aria-label={t("annot.tools")}>
      <div class="tool-group">
        <For each={TOOLS}>
          {(x) => (
            <button
              type="button"
              class="tool"
              classList={{ active: tool() === x.tool }}
              aria-pressed={tool() === x.tool}
              title={x.key ? `${toolLabel(x.tool)} (${x.key})` : toolLabel(x.tool)}
              onClick={() => setTool(tool() === x.tool ? "select" : x.tool)}
            >
              {toolLabel(x.tool)}
            </button>
          )}
        </For>
      </div>
      <div class="swatches" role="radiogroup" aria-label={t("annot.color")}>
        <For each={usesMarkColors() ? COLORS : INK_COLORS}>
          {(c) => {
            const on = (): boolean => (usesMarkColors() ? markColor() : inkColor()) === c;
            return <button type="button" class="swatch" role="radio" aria-checked={on()} aria-label={c} style={{ background: c }} onClick={() => (usesMarkColors() ? setMarkColor(c) : setInkColor(c))} />;
          }}
        </For>
      </div>
      <select class="ep-select" aria-label={t("annot.category")} value={category() ?? ""} onChange={(e) => setCategory((e.currentTarget.value || null) as Category | null)}>
        <option value="">{t("annot.no_category")}</option>
        <For each={CATEGORIES}>{(c) => <option value={c}>{categoryLabel(c)}</option>}</For>
      </select>
      <Show when={["ink", "rectangle", "ellipse", "line", "arrow"].includes(tool())}>
        <select class="ep-select" aria-label={t("annot.width")} value={strokeWidth()} onChange={(e) => setStrokeWidth(Number(e.currentTarget.value))}>
          <For each={[0.75, 1.5, 3, 5]}>{(w) => <option value={w}>{w} pt</option>}</For>
        </select>
      </Show>
      <span class="spacer" />
      <button type="button" class="icon" disabled={!set()?.can_undo} title={`${t("annot.undo")} (Ctrl+Z)`} aria-label={t("annot.undo")} onClick={() => void undo(props.paper)}>
        ↶
      </button>
      <button type="button" class="icon" disabled={!set()?.can_redo} title={`${t("annot.redo")} (Ctrl+Y)`} aria-label={t("annot.redo")} onClick={() => void redo(props.paper)}>
        ↷
      </button>
      <span class="saved muted small" aria-live="polite">
        {savedAt() ? t("annot.saved") : ""}
      </span>
      <button type="button" class="ep-button" data-variant={set()?.unsaved ? "primary" : "default"} disabled={props.saving || !set()?.unsaved} title={`${t("annot.save_pdf_hint")} (Ctrl+S)`} onClick={props.onSave}>
        <Show when={set()?.unsaved}>
          <span class="dot" aria-hidden="true" />
        </Show>
        {props.saving ? t("annot.saving_pdf") : set()?.unsaved ? t("annot.save_pdf") : t("annot.in_pdf")}
      </button>
    </div>
  );
}

export { TOOLS };
