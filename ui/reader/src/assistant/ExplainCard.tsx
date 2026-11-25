// The Explain card (design language §11, §17.13): next to what is explained, never over it; what
// code knows is there at once, the text streams below with Stop; what the check found, in words;
// chips to the passages; simpler or deeper; copy and pin to notes. The consent dialog for looking
// at pictures is here too.

import type { Basis, Evidence, ExplainDepth, ExplainStatus, UnitsCheck } from "@electropaper/bindings";
import { Button, Card, Dialog, IconButton, showToast, t } from "@electropaper/kit";
import { For, type JSX, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";

import { setSettingsOpen } from "../intel/state";
import { quoteToNotes } from "../notes/state";
import { canLook, closeExplain, explainAgain, explainCard, explainRunning, isSelection, languageModels, lookAt, lookQuestion, stopExplain } from "./explain";

const W = 420;
/** Height kept free for the card when choosing a side. */
const ROOM = 260;
/** The selection bar may stand under a selection near the top of the window. */
const BAR = 48;
/** The top bar and the status line. */
const TOP = 56;
const BOTTOM = 34;

/** Below, above, right or left of what is explained; else in the corner. */
function place(at: DOMRect, selection: boolean, win: { w: number; h: number }): JSX.CSSProperties {
  const w = Math.min(W, win.w - 16);
  const h = win.h;
  const left = Math.max(8, Math.min(win.w - w - 8, at.left + at.width / 2 - w / 2));
  const barBelow = selection && at.top - BAR < 60;
  const top = at.bottom + 8 + (barBelow ? BAR : 0);
  const bottom = at.top - 8 - (selection && !barBelow ? BAR : 0);
  const box = (c: JSX.CSSProperties, room: number): JSX.CSSProperties => ({ ...c, width: `${w}px`, "--room": `${Math.max(120, Math.round(room))}px` });
  if (top >= TOP && top + ROOM <= h - BOTTOM) return box({ left: `${left}px`, top: `${top}px` }, h - BOTTOM - top);
  if (bottom <= h - BOTTOM && bottom - ROOM >= TOP) return box({ left: `${left}px`, bottom: `${h - bottom}px` }, bottom - TOP);
  const y = Math.max(TOP, Math.min(at.top, h - BOTTOM - ROOM));
  if (at.right + 8 + w <= win.w - 8) return box({ left: `${at.right + 8}px`, top: `${y}px` }, h - BOTTOM - y);
  if (at.left - 8 - w >= 8 && at.left <= win.w) return box({ left: `${at.left - 8 - w}px`, top: `${y}px` }, h - BOTTOM - y);
  return box({ right: "16px", bottom: `${BOTTOM + 8}px` }, h - BOTTOM - TOP - 8);
}

const FROM: Basis[] = ["caption", "text", "mentions", "words", "cells", "formula", "picture"];

/** "From the caption and the text." */
function basedOn(basis: Basis[]): string {
  const all = FROM.filter((b) => basis.includes(b)).map((b) => t(`explain.from.${b}`));
  const last = all.pop();
  if (!last) return "";
  return t("explain.from", { sources: all.length > 0 ? t("explain.from.and", { list: all.join(", "), last }) : last });
}

function units(u: UnitsCheck): string {
  if (u.result === "consistent") return t("explain.units.consistent");
  if (u.result === "inconsistent") return t("explain.units.inconsistent", { left: u.sides[0] ?? "", right: u.sides[1] ?? "" });
  if (u.unread) return t("explain.units.unread");
  return u.missing.length > 0 ? t("explain.units.unknown", { symbols: u.missing.join(", ") }) : "";
}

export function ExplainHost(props: { paper: string; goTo: (page: number, y: number) => void }): JSX.Element {
  let box: HTMLDivElement | undefined;
  // The card stays inside a window that changes its size.
  const size = () => ({ w: window.innerWidth, h: window.innerHeight });
  const [win, setWin] = createSignal(size());
  const resized = (): void => void setWin(size());
  onMount(() => {
    resized();
    window.addEventListener("resize", resized);
    onCleanup(() => window.removeEventListener("resize", resized));
    const down = (e: PointerEvent): void => {
      const el = e.target as HTMLElement;
      if (explainCard() && box && !box.contains(el) && !el.closest(".selection-bar, .scroller, .ep-dialog, .ep-scrim, .ep-menu")) closeExplain();
    };
    const key = (e: KeyboardEvent): void => {
      // A dialog or a menu that is open closes first.
      if (e.key === "Escape" && !e.defaultPrevented && explainCard() && !document.querySelector(".ep-scrim, .ep-menu")) {
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
  const answer = (): { model: string; status: ExplainStatus; evidence: Evidence[] } | undefined => c()?.result ?? c()?.about;
  const modelName = (): string => {
    const id = answer()?.model;
    return languageModels()?.models.find((m) => m.id === id)?.name ?? id ?? "";
  };
  const done = (): boolean => c()?.phase === "done";
  const status = (): string => {
    switch (c()?.phase) {
      case "looking":
        return "";
      case "starting":
        return t("explain.starting");
      case "draft":
        return t("explain.writing");
      case "checking":
        return t("explain.checking");
      case "stopped":
        return t("explain.stopped");
      case "done":
        return c()?.text ? t(`explain.${answer()?.status ?? "unchecked"}`) : "";
      default:
        return "";
    }
  };
  /** One chip per page, in page order. */
  const chips = (): Evidence[] => {
    const seen = new Map<number, Evidence>();
    for (const e of answer()?.evidence ?? []) if (!seen.has(e.page)) seen.set(e.page, e);
    return [...seen.values()].sort((a, b) => a.page - b.page);
  };
  const needsModel = (): boolean => ["no_model", "no_runtime"].includes(c()?.error?.kind ?? "");
  /** The paper says nothing about it. */
  const nothing = (): string => {
    const k = c()?.kind;
    if (!done() || c()?.text || !c()?.about) return "";
    return t(k === "figure" || k === "table" ? `explain.nothing.${k}` : "explain.nothing.other");
  };
  const thing = () => {
    const s = c()?.subject;
    return s && !isSelection(s) ? s : null;
  };
  const isFigure = (): boolean => c()?.kind === "figure";
  const depths = (): ExplainDepth[] => (["simpler", "normal", "deeper"] as const).filter((d) => d !== c()?.depth);
  const leftOut = (): number => c()?.about?.left_out ?? 0;
  const anyAnswer = (): boolean => !!c()?.text && !explainRunning();

  return (
    <>
      <Show when={c()}>
        {(s) => (
          <div class="peek explain" ref={box} style={place(s().at, s().kind === "selection", win())}>
            <Card title={s().title} icon="sparkles" onClose={closeExplain} closeLabel={t("common.close")}>
              <Show
                when={s().phase !== "failed"}
                fallback={
                  <div class="explain-failed">
                    <p>{needsModel() ? t("explain.no_model") : (s().error?.message ?? "")}</p>
                    <Show when={needsModel()}>
                      <Button icon="settings" onClick={() => (closeExplain(), setSettingsOpen(true))}>{t("explain.open_settings")}</Button>
                    </Show>
                  </div>
                }
              >
                <Show when={s().symbols.length > 0}>
                  <table class="explain-symbols" aria-label={t("explain.symbols")}>
                    <thead>
                      <tr>
                        <th>{t("explain.symbol")}</th>
                        <th>{t("explain.meaning")}</th>
                        <th>{t("explain.unit")}</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      <For each={s().symbols}>
                        {(y) => (
                          <tr>
                            <td>{y.shown}</td>
                            <td classList={{ muted: !y.meaning }}>{y.meaning ?? t("explain.not_explained")}</td>
                            <td>{y.unit ?? ""}</td>
                            <td>
                              <Show when={y.page}>
                                {(p) => (
                                  <button type="button" class="loc-chip" data-tip={y.quote ?? undefined} onClick={() => props.goTo(p(), y.y ?? 0)}>
                                    {t("explain.page_chip", { n: p() })}
                                  </button>
                                )}
                              </Show>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </Show>
                <Show when={s().text}>
                  <p class="explain-text">{s().text}</p>
                </Show>
                <Show when={nothing()}>
                  <p class="explain-text">{nothing()}</p>
                </Show>
                <Show when={status()}>
                  <p class="explain-status muted" role="status">
                    <Show when={explainRunning() && !s().looking}>
                      <span class="spinner" aria-hidden="true" />
                    </Show>
                    {status()}
                  </p>
                </Show>
                <Show when={done() && s().about?.units}>
                  {(u) => (
                    <Show when={units(u())}>
                      <p class="explain-status" data-units={u().result}>{units(u())}</p>
                    </Show>
                  )}
                </Show>
                <Show when={done() && leftOut() > 0}>
                  <p class="explain-status muted">{leftOut() === 1 ? t("explain.left_out.one") : t("explain.left_out.many", { n: leftOut() })}</p>
                </Show>
                <Show when={done() && s().text && s().about ? s().about : null}>
                  {(a) => (
                    <p class="explain-status muted">
                      {basedOn(a().based_on)}
                      <Show when={isFigure() && !s().seen && !s().looking}> {t("explain.not_looked")}</Show>
                    </p>
                  )}
                </Show>
                <Show when={done() && s().text && answer()}>
                  <p class="explain-status muted">{t("explain.generated", { model: modelName() })}</p>
                </Show>
                <Show when={done() && answer() && answer()?.status !== "unchecked" && chips().length > 0}>
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
                <Show when={s().looking || s().seen}>
                  <div class="explain-seen">
                    <strong>{t("look.seen")}</strong>
                    <Show when={s().looking?.text || s().seen?.text}>{(x) => <p>{x()}</p>}</Show>
                    <p class="explain-status muted" role="status">
                      <Show when={s().looking} fallback={t("look.seen_by", { model: s().seen?.model ?? "" })}>
                        <span class="spinner" aria-hidden="true" />
                        {t("look.looking")}
                      </Show>
                    </p>
                  </div>
                </Show>
                <Show when={s().error}>
                  {(e) => <p class="explain-status">{t("look.failed", { reason: e().message })}</p>}
                </Show>
                <div class="row explain-actions">
                  <Show when={explainRunning()}>
                    <Button variant="quiet" icon="x" onClick={stopExplain}>{t("explain.stop")}</Button>
                  </Show>
                  <Show when={!explainRunning() && isFigure() && canLook() ? thing() : null}>
                    {(th) => (
                      <Button variant="quiet" icon="eye" onClick={() => void lookAt(th())}>
                        {t(s().seen ? "look.again" : "look.action")}
                      </Button>
                    )}
                  </Show>
                  <Show when={done() && anyAnswer()}>
                    <For each={depths()}>
                      {(d) => (
                        <Button variant="quiet" onClick={() => void explainAgain(d)}>
                          {t(`explain.${d}`)}
                        </Button>
                      )}
                    </For>
                  </Show>
                  <Show when={anyAnswer()}>
                    <IconButton icon="copy" size="sm" label={t("annot.copy")} onClick={() => void navigator.clipboard.writeText(s().text).then(() => showToast(t("explain.copied"))).catch(() => {})} />
                    <IconButton icon="notebook-pen" size="sm" label={t("notes.add_to_notes")} onClick={() => void quoteToNotes(s().paper, { text: s().text, page: s().page, y: s().y })} />
                  </Show>
                </div>
              </Show>
            </Card>
          </div>
        )}
      </Show>
      <LookDialog />
    </>
  );
}

/** Asks once per paper whether its pictures may be looked at (§17.13). */
function LookDialog(): JSX.Element {
  return (
    <Show when={lookQuestion()} keyed>
      {(q) => (
        <Dialog
          title={t(q.total > 1 ? "look.title" : "look.title_one")}
          onCancel={() => q.answer(null)}
          actions={
            <Show
              when={q.total > 1}
              fallback={
                <>
                  <Button onClick={() => q.answer(null)}>{t("intel.not_now")}</Button>
                  <Button variant="primary" onClick={() => q.answer("this")}>{t("look.yes")}</Button>
                </>
              }
            >
              <Button onClick={() => q.answer(null)}>{t("intel.not_now")}</Button>
              <Button onClick={() => q.answer("all")}>{t("look.all", { n: q.total })}</Button>
              <Button variant="primary" data-autofocus onClick={() => q.answer("this")}>{t("look.this")}</Button>
            </Show>
          }
        >
          <p>{t("look.body", { model: languageModels()?.vision?.name ?? "" })}</p>
        </Dialog>
      )}
    </Show>
  );
}
