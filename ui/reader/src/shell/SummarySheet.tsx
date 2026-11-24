// The summary of a paper (design language §17.11): what it is, its subjects, and every section with its
// own summary and tags. It can be read while it is being made.

import type { Digest, Paper, SectionDigest } from "@electropaper/bindings";
import { Button, Dialog, type MessageKey, Skeleton, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";

import { digestStatus, makeDigests, stopDigests } from "../assistant/ask";
import { canExplain, languageModels, refreshLanguageModels } from "../assistant/explain";
import { bus, onBusEvent } from "../host";
import { setSettingsOpen } from "../intel/state";
import { titleOf } from "../library";

export function SummarySheet(props: { paper: Paper; onClose: () => void; onError: (e: unknown) => void; onOpenAt: (page: number, y: number) => void; onAsk: (question: string) => void }): JSX.Element {
  const [digest, setDigest] = createSignal<Digest | null>(null);
  const [loaded, setLoaded] = createSignal(false);
  let open = true;
  const load = async (): Promise<void> => {
    const d = await bus("knowledge.get_digest", { paper: props.paper.id });
    if (!open) return;
    setDigest(d ?? null);
    setLoaded(true);
  };
  const safe = (f: () => Promise<unknown>): void => void f().catch(props.onError);
  onMount(async () => {
    onCleanup(() => {
      open = false;
    });
    safe(refreshLanguageModels);
    safe(load);
    const stop = await onBusEvent("knowledge.digested", (e) => {
      if ((e.data as { paper: string }).paper === props.paper.id) safe(load);
    });
    if (open) onCleanup(stop);
    else stop();
  });
  const making = (): boolean => !!digestStatus()?.running && (digestStatus()?.paper === props.paper.id || (digestStatus()?.next ?? []).includes(props.paper.id));
  const full = (): boolean => digest()?.level === "full";
  const make = (force: boolean): void => safe(() => makeDigests({ full: false, papers: [props.paper.id], force }));
  const modelName = (): string => languageModels()?.models.find((m) => m.id === digest()?.model)?.name ?? digest()?.model ?? "";
  const pages = (s: SectionDigest): string => (s.pages[0] === s.pages[1] ? t("explain.page_chip", { n: s.pages[0] }) : t("summary.pages", { from: s.pages[0], to: s.pages[1] }));
  const at = (page: number): void => {
    props.onClose();
    props.onOpenAt(page, 0);
  };

  return (
    <Dialog
      size="lg"
      title={titleOf(props.paper)}
      onCancel={props.onClose}
      actions={
        <>
          <Show
            when={making()}
            fallback={
              <Show
                when={canExplain()}
                fallback={
                  <Button icon="settings" onClick={() => (props.onClose(), setSettingsOpen(true))}>
                    {t("ask.no_model")}
                  </Button>
                }
              >
                <Button variant={full() ? undefined : "primary"} icon="sparkles" onClick={() => make(full() && !!digest()?.complete)}>
                  {full() ? (digest()?.complete ? t("summary.again") : t("summary.go_on")) : t("summary.make")}
                </Button>
              </Show>
            }
          >
            <Button onClick={() => safe(stopDigests)}>{t("explain.stop")}</Button>
          </Show>
          <Button onClick={props.onClose}>{t("common.close")}</Button>
        </>
      }
    >
      <div class="summary-sheet">
        <Show when={making()}>
          <p class="muted" role="status">
            <span class="spinner" aria-hidden="true" /> {digestStatus()?.paper === props.paper.id ? t("summary.making", { step: digestStatus()?.step ?? "" }) : t("summary.waiting")}
          </p>
        </Show>
        <Show when={loaded()} fallback={<Skeleton lines={4} />}>
          <Show when={digest()} fallback={<p class="muted">{t("summary.none")}</p>}>
            {(d) => (
              <>
                <Show when={!full()}>
                  <p class="muted">{t("summary.basic")}</p>
                </Show>
                <Show when={d().summary}>
                  <p>{d().summary}</p>
                </Show>
                <p class="summary-tags">
                  <Show when={d().kind}>
                    <strong>{d().kind}</strong>
                  </Show>
                  <For each={d().topics}>{(w) => <span class="tag small">{w}</span>}</For>
                </p>
                <Show when={(d().questions ?? []).length > 0}>
                  <h4>{t("summary.questions")}</h4>
                  <For each={d().questions}>
                    {(q) => (
                      <button type="button" class="ask-example" onClick={() => props.onAsk(q)}>
                        {q}
                      </button>
                    )}
                  </For>
                </Show>
                <h4>{t("summary.sections", { n: d().sections.length })}</h4>
                <For each={d().sections}>
                  {(s) => (
                    <section class="summary-section" classList={{ inner: s.level > 1 }}>
                      <div class="summary-head">
                        <strong>{s.title}</strong>
                        <button type="button" class="loc-chip" onClick={() => at(s.pages[0])}>
                          {pages(s)}
                        </button>
                        <Show when={s.role && s.role !== "other"}>
                          <span class="muted small">{t(`summary.role.${s.role}` as MessageKey)}</span>
                        </Show>
                        <Show when={s.source === "quoted"}>
                          <span class="muted small">{t("summary.quoted")}</span>
                        </Show>
                      </div>
                      <Show when={s.summary}>
                        <p>{s.summary}</p>
                      </Show>
                      <Show when={(s.tags ?? []).length > 0 || (!s.summary && (s.terms ?? []).length > 0)}>
                        <p class="summary-tags">
                          <For each={(s.tags ?? []).length > 0 ? s.tags : s.terms}>{(w) => <span class="tag small">{w}</span>}</For>
                        </p>
                      </Show>
                    </section>
                  )}
                </For>
                <Show when={d().model}>
                  <p class="muted explain-status">
                    {t("explain.generated", { model: modelName() })} · {t("summary.checked")}
                  </p>
                </Show>
              </>
            )}
          </Show>
        </Show>
      </div>
    </Dialog>
  );
}
