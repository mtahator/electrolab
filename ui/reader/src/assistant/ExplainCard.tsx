// The Explain card (design language §11): next to the selection, never over it; streamed text
// with Stop; what the check found, in words; chips to the passages; copy and pin to notes.

import type { Evidence } from "@electropaper/bindings";
import { Button, Card, IconButton, showToast, t } from "@electropaper/kit";
import { For, type JSX, Show, createEffect, onCleanup, onMount } from "solid-js";

import { setSettingsOpen } from "../intel/state";
import { quoteToNotes } from "../notes/state";
import { closeExplain, explainCard, explainRunning, languageModels, stopExplain } from "./explain";

const W = 420;
/** Height kept free for the card when choosing a side. */
const ROOM = 260;
/** The selection bar may stand under a selection near the top of the window. */
const BAR = 48;

function place(at: DOMRect): JSX.CSSProperties {
  const left = Math.max(8, Math.min(window.innerWidth - W - 8, at.left + at.width / 2 - W / 2));
  const barBelow = at.top - BAR < 60;
  const below = at.bottom + 8 + (barBelow ? BAR : 0);
  if (below + ROOM <= window.innerHeight - 30) return { left: `${left}px`, top: `${below}px`, width: `${W}px` };
  return { left: `${left}px`, bottom: `${window.innerHeight - at.top + 8 + (barBelow ? 0 : BAR)}px`, width: `${W}px` };
}

export function ExplainHost(props: { paper: string; goTo: (page: number, y: number) => void }): JSX.Element {
  let box: HTMLDivElement | undefined;
  onMount(() => {
    const down = (e: PointerEvent): void => {
      const el = e.target as HTMLElement;
      if (explainCard() && box && !box.contains(el) && !el.closest(".selection-bar, .scroller, .ep-dialog")) closeExplain();
    };
    const key = (e: KeyboardEvent): void => {
      if (e.key === "Escape" && explainCard()) {
        e.stopPropagation();
        closeExplain();
      }
    };
    // A click (not a drag or a scroll) on the page closes it.
    const click = (e: MouseEvent): void => {
      const el = e.target as HTMLElement;
      if (e.detail === 1 && explainCard() && el.closest(".scroller")) closeExplain();
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("click", click, true);
    window.addEventListener("keydown", key, true);
    onCleanup(() => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("click", click, true);
      window.removeEventListener("keydown", key, true);
    });
  });
  // Leaving the paper closes its card (and stops the answer).
  createEffect(() => {
    const c = explainCard();
    if (c && c.paper !== props.paper) closeExplain();
  });

  const c = explainCard;
  const modelName = (): string => {
    const id = c()?.result?.model;
    return languageModels()?.models.find((m) => m.id === id)?.name ?? id ?? "";
  };
  const status = (): string => {
    switch (c()?.phase) {
      case "starting":
        return t("explain.starting");
      case "draft":
        return t("explain.writing");
      case "checking":
        return t("explain.checking");
      case "stopped":
        return t("explain.stopped");
      case "done":
        return t(`explain.${c()?.result?.status ?? "unchecked"}`);
      default:
        return "";
    }
  };
  /** One chip per page, in page order. */
  const chips = (): Evidence[] => {
    const seen = new Map<number, Evidence>();
    for (const e of c()?.result?.evidence ?? []) if (!seen.has(e.page)) seen.set(e.page, e);
    return [...seen.values()].sort((a, b) => a.page - b.page);
  };
  const needsModel = (): boolean => ["no_model", "no_runtime"].includes(c()?.error?.kind ?? "");

  return (
    <Show when={c()}>
      {(s) => (
        <div class="peek explain" ref={box} style={place(s().at)}>
          <Card title={s().selection} icon="sparkles" onClose={closeExplain} closeLabel={t("common.close")}>
            <Show when={s().phase !== "failed"} fallback={
              <div class="explain-failed">
                <p>{needsModel() ? t("explain.no_model") : (s().error?.message ?? "")}</p>
                <Show when={needsModel()}>
                  <Button icon="settings" onClick={() => (closeExplain(), setSettingsOpen(true))}>{t("explain.open_settings")}</Button>
                </Show>
              </div>
            }>
              <Show when={s().text}>
                <p class="explain-text">{s().text}</p>
              </Show>
              <p class="explain-status muted" role="status">
                <Show when={explainRunning()}>
                  <span class="spinner" aria-hidden="true" />
                </Show>
                {status()}
              </p>
              <Show when={s().result}>
                <p class="explain-status muted">{t("explain.generated", { model: modelName() })}</p>
              </Show>
              <Show when={s().result && (s().result?.status !== "unchecked") && (s().result?.evidence.length ?? 0) > 0}>
                <div class="explain-evidence">
                  <For each={chips()}>
                    {(e) => (
                      <button type="button" class="loc-chip" data-tip={e.quote} onClick={() => props.goTo(e.page, e.y)}>
                        {t("explain.page_chip", { n: e.page })}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
              <div class="row explain-actions">
                <Show when={explainRunning()}>
                  <Button variant="quiet" icon="x" onClick={stopExplain}>{t("explain.stop")}</Button>
                </Show>
                <Show when={!explainRunning() && s().text}>
                  <IconButton icon="copy" size="sm" label={t("annot.copy")} onClick={() => void navigator.clipboard.writeText(s().text).then(() => showToast(t("explain.copied"))).catch(() => {})} />
                  <IconButton icon="notebook-pen" size="sm" label={t("notes.add_to_notes")} onClick={() => void quoteToNotes(s().paper, { text: s().text, page: s().page, y: s().y })} />
                </Show>
              </div>
            </Show>
          </Card>
        </div>
      )}
    </Show>
  );
}
