// Asking the library (design language §17.11): a question or a description; the papers it can be in
// and the nearest places, filled as they are found. Works with words alone when there is no model.

import type { Paper, Place } from "@electropaper/bindings";
import { Button, EmptyState, Icon, type MessageKey, t } from "@electropaper/kit";
import { For, type JSX, Show, createMemo, createSignal, onMount } from "solid-js";

import { ask, askError, askResult, askStep, asking, digestStatus, digests, enterAsk, library, makeDigests, question, setQuestion, stopAsk, stopDigests } from "../assistant/ask";
import { canExplain, languageModels } from "../assistant/explain";
import { ObjectPicture } from "../intel/ObjectPicture";
import { setSettingsOpen } from "../intel/state";
import { titleOf } from "../library";
import { SummarySheet } from "./SummarySheet";

/** Places of a paper shown before "more". */
const FEW = 4;

export function AskView(props: { onOpenAt: (p: Paper, page: number, y: number) => void; onError: (e: unknown) => void }): JSX.Element {
  const safe = (f: () => Promise<unknown>): void => void f().catch(props.onError);
  onMount(() => safe(enterAsk));
  const [summaryOf, setSummaryOf] = createSignal<Paper | null>(null);
  const [more, setMore] = createSignal<ReadonlySet<string>>(new Set());
  const run = (text: string): void => {
    setMore(new Set<string>());
    void ask(text);
  };
  const open = (p: Place): void => {
    const paper = library().get(p.paper);
    if (paper) props.onOpenAt(paper, p.page, p.y);
  };
  const nearest = createMemo(() => (askResult()?.places ?? []).filter((p) => p.nearest));
  /** Papers with their places: the related ones in their order, then the others. */
  const groups = createMemo(() => {
    const r = askResult();
    if (!r) return [];
    const ids = [...new Set([...r.related.map((x) => x.paper), ...r.places.map((x) => x.paper)])];
    return ids.map((id) => ({ id, why: r.related.find((x) => x.paper === id)?.why ?? null, places: r.places.filter((x) => x.paper === id && !x.nearest) })).filter((g) => library().has(g.id));
  });
  /** The words that stand in something that was found. */
  const wordsMet = createMemo(() => {
    const r = askResult();
    const met = new Set((r?.places ?? []).flatMap((p) => p.terms));
    return (r?.terms ?? []).filter((w) => met.has(w));
  });
  const others = createMemo(() => (askResult()?.others ?? []).filter((o) => library().has(o.paper) && !groups().some((g) => g.id === o.paper)));
  const modelName = (): string => languageModels()?.models.find((m) => m.id === askResult()?.model)?.name ?? askResult()?.model ?? "";
  const written = (): number => [...digests().values()].filter((d) => d.level === "full" && d.complete).length;

  const Place_ = (q: { place: Place; picture: boolean }): JSX.Element => {
    const paper = (): Paper | undefined => library().get(q.place.paper);
    const rect = (): { page: number; x0: number; y0: number; x1: number; y1: number } | null => {
      const r = q.place.rect;
      return r ? { page: q.place.page, x0: r[0] ?? 0, y0: r[1] ?? 0, x1: r[2] ?? 0, y1: r[3] ?? 0 } : null;
    };
    return (
      <button type="button" class="ask-place" classList={{ near: q.place.nearest }} onClick={() => open(q.place)}>
        <span class="ask-place-head">
          <strong>{q.place.label ?? q.place.section ?? t("ask.passage")}</strong>
          <span class="muted">{t("explain.page_chip", { n: q.place.page })}</span>
        </span>
        <Show when={q.picture && rect() && paper()}>{(p) => <ObjectPicture file={p().file} rect={rect() as NonNullable<ReturnType<typeof rect>>} label={q.place.label} equation={q.place.kind === "equation"} />}</Show>
        <span class="ask-quote">
          <Marked text={q.place.quote} words={q.place.terms} />
        </span>
        <Show when={q.picture}>
          <span class="muted small ask-from">{titleOf(paper())}</span>
        </Show>
      </button>
    );
  };

  return (
    <section class="library-main ask">
      <form
        class="ask-box"
        onSubmit={(e) => {
          e.preventDefault();
          run(question());
        }}
      >
        <textarea
          class="ep-input"
          rows={2}
          aria-label={t("ask.title")}
          placeholder={t("ask.placeholder")}
          value={question()}
          ref={(el) => queueMicrotask(() => el.focus())}
          onInput={(e) => setQuestion(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
              e.preventDefault();
              run(question());
            } else if (e.key === "Escape" && asking()) {
              e.stopPropagation();
              stopAsk();
            }
          }}
        />
        <Show
          when={asking()}
          fallback={
            <Button variant="primary" icon="search" disabled={!question().trim()} onClick={() => run(question())}>
              {t("ask.search")}
            </Button>
          }
        >
          <Button icon="x" onClick={stopAsk}>
            {t("explain.stop")}
          </Button>
        </Show>
      </form>

      <div class="ask-summaries" role="status">
        <Show
          when={digestStatus()?.running}
          fallback={
            <>
              <span class="muted">{t("ask.summaries", { n: written(), total: library().size })}</span>
              <Show when={library().size > written()}>
                <Show
                  when={canExplain()}
                  fallback={
                    <Button variant="quiet" icon="settings" onClick={() => setSettingsOpen(true)}>
                      {t("ask.no_model")}
                    </Button>
                  }
                >
                  <Button variant="quiet" icon="sparkles" onClick={() => safe(() => makeDigests({ full: true }))}>
                    {t("ask.make_all")}
                  </Button>
                </Show>
              </Show>
            </>
          }
        >
          <span class="spinner" aria-hidden="true" />
          <span class="muted ask-making">
            {t("ask.making", { title: digestStatus()?.current ?? "" })}
            {digestStatus()?.step ? ` · ${digestStatus()?.step}` : ""}
            {digestStatus()?.full ? ` · ${t("ask.of_papers", { n: digestStatus()?.done ?? 0, total: digestStatus()?.total ?? 0 })}` : ""}
          </span>
          <Button variant="quiet" onClick={() => safe(stopDigests)}>
            {t("explain.stop")}
          </Button>
        </Show>
        <Show when={digestStatus()?.error && !digestStatus()?.running}>
          <span class="error">{digestStatus()?.error}</span>
        </Show>
      </div>

      <div class="ask-results">
        <Show when={askStep()} fallback={<Empty onPick={run} />}>
          <Show when={asking() || askStep() === "stopped"}>
            <p class="muted ask-status" role="status">
              <Show when={asking()}>
                <span class="spinner" aria-hidden="true" />
              </Show>{" "}
              {t(`ask.step.${askStep()}` as MessageKey)}
            </p>
          </Show>
          <Show when={askStep() === "failed"}>
            <p class="error">{askError()}</p>
          </Show>
          <Show when={askResult()}>
            {(r) => (
              <>
                <Show when={wordsMet().length > 0}>
                  <p class="muted ask-words">
                    {t("ask.words")} <For each={wordsMet()}>{(w) => <span class="tag small">{w}</span>}</For>
                  </p>
                </Show>
                <Show when={nearest().length > 0}>
                  <h3>{t("ask.nearest")}</h3>
                  <div class="ask-near">
                    <For each={nearest()}>{(p) => <Place_ place={p} picture />}</For>
                  </div>
                </Show>
                <Show when={groups().length > 0}>
                  <h3>{t("ask.papers")}</h3>
                </Show>
                <For each={groups()}>
                  {(g) => (
                    <section class="ask-paper">
                      <div class="ask-paper-head">
                        <button type="button" class="ask-title" onClick={() => props.onOpenAt(library().get(g.id) as Paper, 1, 0)}>
                          {titleOf(library().get(g.id))}
                        </button>
                        <Button variant="quiet" icon="list-tree" onClick={() => setSummaryOf(library().get(g.id) ?? null)}>
                          {t("summary.title")}
                        </Button>
                      </div>
                      <Show when={g.why}>
                        <p class="muted">{g.why}</p>
                      </Show>
                      <For each={more().has(g.id) ? g.places : g.places.slice(0, FEW)}>{(p) => <Place_ place={p} picture={false} />}</For>
                      <Show when={g.places.length > FEW && !more().has(g.id)}>
                        <Button variant="quiet" onClick={() => setMore((m) => new Set([...m, g.id]))}>
                          {t("ask.more", { n: g.places.length - FEW })}
                        </Button>
                      </Show>
                    </section>
                  )}
                </For>
                <Show when={others().length > 0}>
                  <h3>{t("ask.others")}</h3>
                  <ul class="ask-others">
                    <For each={others()}>
                      {(o) => (
                        <li>
                          <button type="button" class="ask-title" onClick={() => props.onOpenAt(library().get(o.paper) as Paper, 1, 0)}>
                            {titleOf(library().get(o.paper))}
                          </button>{" "}
                          <span class="muted">{t("ask.hits", { n: o.hits })}</span>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
                <Show when={askStep() === "done" && groups().length === 0 && others().length === 0}>
                  <EmptyState icon="search" text={t("ask.nothing")} />
                </Show>
                <Show when={askStep() === "done"}>
                  <p class="muted explain-status">
                    <Icon name={r().used_model ? "sparkles" : "search"} size={14} /> {r().used_model ? t("ask.by_model", { model: modelName() }) : t("ask.by_words")}
                  </p>
                </Show>
              </>
            )}
          </Show>
        </Show>
      </div>
      <Show when={summaryOf()} keyed>
        {(p) => (
          <SummarySheet
            paper={p}
            onClose={() => setSummaryOf(null)}
            onError={props.onError}
            onOpenAt={(page, y) => props.onOpenAt(p, page, y)}
            onAsk={(q) => {
              setSummaryOf(null);
              run(q);
            }}
          />
        )}
      </Show>
    </section>
  );
}

function Empty(props: { onPick: (q: string) => void }): JSX.Element {
  return (
    <div class="ask-empty">
      <p>{t("ask.empty")}</p>
      <For each={[t("ask.example_1"), t("ask.example_2"), t("ask.example_3")]}>
        {(q) => (
          <button type="button" class="ask-example" onClick={() => props.onPick(q)}>
            {q}
          </button>
        )}
      </For>
    </div>
  );
}

/** A quote with the words that were searched set off. */
function Marked(props: { text: string; words: string[] }): JSX.Element {
  const parts = createMemo(() => {
    const words = [...new Set(props.words.flatMap((w) => w.split(/[\s-]+/)).filter((w) => w.length > 2))].sort((a, b) => b.length - a.length);
    if (words.length === 0) return [props.text];
    const escaped = words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    return props.text.split(new RegExp(`(${escaped.join("|")})`, "gi"));
  });
  // A split with a group gives the matches at the odd places.
  return <For each={parts()}>{(x, i) => (i() % 2 === 1 ? <mark>{x}</mark> : <>{x}</>)}</For>;
}
