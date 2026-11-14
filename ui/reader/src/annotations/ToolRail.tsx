// The tool rail (guideline §2, P1.MD.FRM.2): annotation tools as icons with tooltips and keys,
// grouped; the chosen tool's options (colour, category, line width) in a small flyout next to it.
// At the bottom: undo, redo, show/hide annotations and Save into PDF.

import type { Category } from "@electropaper/bindings";
import { IconButton, type IconName, Segmented, Swatches, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";

import { categoryLabel } from "./Editor";
import {
  CATEGORIES,
  COLORS,
  INK_COLORS,
  type Tool,
  annotationsOf,
  category,
  colorNames,
  inkColor,
  isMarkup,
  markColor,
  marksHidden,
  redo,
  setCategory,
  setInkColor,
  setMarkColor,
  setMarksHidden,
  setStrokeWidth,
  setTool,
  strokeWidth,
  tool,
  undo,
} from "./state";

interface ToolDef {
  tool: Tool;
  icon: IconName;
  key?: string;
}

export const TOOL_GROUPS: ToolDef[][] = [
  [{ tool: "select", icon: "mouse-pointer-2", key: "V" }],
  [
    { tool: "highlight", icon: "highlighter", key: "H" },
    { tool: "underline", icon: "underline", key: "U" },
    { tool: "strike_out", icon: "strikethrough", key: "S" },
    { tool: "squiggly", icon: "spline" },
  ],
  [
    { tool: "note", icon: "message-circle-plus", key: "N" },
    { tool: "free_text", icon: "type", key: "T" },
  ],
  [
    { tool: "ink", icon: "pen-line", key: "D" },
    { tool: "rectangle", icon: "square", key: "R" },
    { tool: "ellipse", icon: "circle", key: "O" },
    { tool: "line", icon: "minus", key: "L" },
    { tool: "arrow", icon: "move-up-right", key: "A" },
  ],
  [{ tool: "eraser", icon: "eraser", key: "E" }],
];

export const toolLabel = (tl: Tool): string => t(tl === "select" ? "annot.tool.select" : tl === "eraser" ? "annot.tool.eraser" : (`annot.kind.${tl}` as Parameters<typeof t>[0]));

const usesMarkColors = (tl: Tool): boolean => isMarkup(tl) || tl === "note";
const DRAWS = new Set<Tool>(["ink", "rectangle", "ellipse", "line", "arrow"]);
const hasOptions = (tl: Tool): boolean => tl !== "select" && tl !== "eraser";

export function ToolRail(props: { paper: string; onSave: () => void; saving: boolean }): JSX.Element {
  let rail!: HTMLDivElement;
  const [flyout, setFlyout] = createSignal<{ top: number } | null>(null);
  const set = (): ReturnType<typeof annotationsOf> => annotationsOf(props.paper);

  const pick = (d: ToolDef, el: HTMLElement): void => {
    const again = tool() === d.tool;
    setTool(d.tool);
    if (!hasOptions(d.tool)) return void setFlyout(null);
    // Choosing a tool shows its options; choosing it again hides them.
    if (again && flyout()) return void setFlyout(null);
    const r = el.getBoundingClientRect();
    setFlyout({ top: r.top });
  };

  onMount(() => {
    const close = (e: PointerEvent): void => {
      const target = e.target as HTMLElement;
      if (!target.closest(".tool-rail, .tool-flyout")) setFlyout(null);
    };
    const esc = (e: KeyboardEvent): void => {
      if (e.key === "Escape") setFlyout(null);
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", esc, true);
    onCleanup(() => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", esc, true);
    });
  });

  const colorOf = (tl: Tool): string | undefined => (tl === "select" || tl === "eraser" ? undefined : usesMarkColors(tl) ? markColor() : inkColor());

  return (
    <>
      <nav class="tool-rail" ref={rail} aria-label={t("annot.tools")} data-tip-side="right">
        <For each={TOOL_GROUPS}>
          {(group) => (
            <div class="tool-group" role="group">
              <For each={group}>
                {(d) => (
                  <IconButton
                    icon={d.icon}
                    label={toolLabel(d.tool)}
                    shortcut={d.key}
                    pressed={tool() === d.tool}
                    swatch={tool() === d.tool ? colorOf(d.tool) : undefined}
                    data-tool={d.tool}
                    onClick={(e) => pick(d, e.currentTarget)}
                  />
                )}
              </For>
            </div>
          )}
        </For>
        <span class="ep-spacer" />
        <div class="tool-group" role="group">
          <IconButton icon="undo-2" label={t("annot.undo")} shortcut="Ctrl+Z" disabled={!set()?.can_undo} onClick={() => void undo(props.paper)} />
          <IconButton icon="redo-2" label={t("annot.redo")} shortcut="Ctrl+Y" disabled={!set()?.can_redo} onClick={() => void redo(props.paper)} />
          <IconButton icon={marksHidden() ? "eye-off" : "eye"} label={marksHidden() ? t("annot.show_all") : t("annot.hide_all")} shortcut="Ctrl+H" pressed={marksHidden()} onClick={() => setMarksHidden(!marksHidden())} />
          <IconButton
            icon="file-down"
            label={set()?.unsaved ? t("annot.save_pdf") : t("annot.in_pdf")}
            shortcut="Ctrl+S"
            dot={!!set()?.unsaved}
            disabled={props.saving || !set()?.unsaved}
            data-action="save-pdf"
            onClick={props.onSave}
          />
        </div>
      </nav>
      <Show when={flyout() && hasOptions(tool())}>
        <div class="tool-flyout ep-card" role="dialog" aria-label={`${toolLabel(tool())}: ${t("annot.options")}`} style={{ top: `${flyout()?.top ?? 0}px` }}>
          <div class="ep-card-body">
            <strong>{toolLabel(tool())}</strong>
            <Swatches
              label={t("annot.color")}
              colors={usesMarkColors(tool()) ? COLORS : INK_COLORS}
              names={colorNames}
              value={usesMarkColors(tool()) ? markColor() : inkColor()}
              onPick={(c) => (usesMarkColors(tool()) ? setMarkColor(c) : setInkColor(c))}
            />
            <Show when={!DRAWS.has(tool()) && tool() !== "free_text"}>
              <select class="ep-select" aria-label={t("annot.category")} value={category() ?? ""} onChange={(e) => setCategory((e.currentTarget.value || null) as Category | null)}>
                <option value="">{t("annot.no_category")}</option>
                <For each={CATEGORIES}>{(c) => <option value={c}>{categoryLabel(c)}</option>}</For>
              </select>
            </Show>
            <Show when={DRAWS.has(tool())}>
              <Segmented
                label={t("annot.width")}
                value={String(strokeWidth())}
                options={["0.75", "1.5", "3", "5"].map((w) => ({ value: w, label: `${w} pt` }))}
                onChange={(w) => setStrokeWidth(Number(w))}
              />
            </Show>
          </div>
        </div>
      </Show>
    </>
  );
}
