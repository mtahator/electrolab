// Side panels (P1.M2.SH.3): contents, page thumbnails, find results, bookmarks.

import type { Bookmark, DocObject, Hit, OutlineItem, Paper } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import { Button, Icon, IconButton, t } from "@electropaper/kit";
import { For, type JSX, Match, Show, Switch, createEffect, createResource, createSignal, on, onCleanup, onMount } from "solid-js";

import { AnnotationsPanel } from "../annotations/AnnotationsPanel";
import { bus, onBusEvent, renderUrl } from "../host";

export type PanelKind = "outline" | "thumbnails" | "find" | "bookmarks" | "annotations";

export interface FindOptions {
  query: string;
  mode: "word_forms" | "exact" | "fuzzy";
  caseSensitive: boolean;
  wholeWord: boolean;
  allTabs: boolean;
}

export interface SidePanelProps {
  kind: PanelKind;
  paper: Paper | null;
  pages: number;
  current: number;
  openPapers: Paper[];
  onGoTo: (page: number, y: number | null) => void;
  /** Show hits (`paper` id → hits), and which one is active. */
  onHits: (hits: Hit[], active: number | null) => void;
  findFocus: number;
}

export function SidePanel(props: SidePanelProps): JSX.Element {
  return (
    <div class="side-panel">
      <Switch>
        <Match when={!props.paper && props.kind !== "find"}>
          <p class="muted">{t("viewer.empty")}</p>
        </Match>
        <Match when={props.kind === "outline"}>
          <Outline paper={props.paper as Paper} onGoTo={props.onGoTo} />
        </Match>
        <Match when={props.kind === "thumbnails"}>
          <Thumbnails paper={props.paper as Paper} pages={props.pages} current={props.current} onGoTo={props.onGoTo} />
        </Match>
        <Match when={props.kind === "find"}>
          <Find paper={props.paper} openPapers={props.openPapers} onHits={props.onHits} focus={props.findFocus} />
        </Match>
        <Match when={props.kind === "annotations"}>
          <AnnotationsPanel paper={props.paper as Paper} current={props.current} onGoTo={props.onGoTo} />
        </Match>
        <Match when={props.kind === "bookmarks"}>
          <Bookmarks paper={props.paper as Paper} current={props.current} onGoTo={props.onGoTo} />
        </Match>
      </Switch>
    </div>
  );
}

function Outline(props: { paper: Paper; onGoTo: (page: number, y: number | null) => void }): JSX.Element {
  const [items] = createResource(() => props.paper.file, (file) => bus("document.outline", { file }));
  const Node = (p: { item: OutlineItem; depth: number }): JSX.Element => {
    const [open, setOpen] = createSignal(p.depth < 1);
    const target = p.item.target;
    const children = p.item.children ?? [];
    return (
      <li>
        <div class="outline-row" style={{ "padding-inline-start": `${p.depth * 12}px` }}>
          <Show when={children.length > 0} fallback={<span class="twisty" />}>
            <button type="button" class="twisty" onClick={() => setOpen(!open())} aria-expanded={open()} aria-label={p.item.title}>
              <Icon name={open() ? "chevron-down" : "chevron-right"} size={14} />
            </button>
          </Show>
          <button
            type="button"
            class="outline-title"
            disabled={!target || target.kind !== "page"}
            onClick={() => target && target.kind === "page" && props.onGoTo(target.page, target.top ?? null)}
          >
            {p.item.title}
          </button>
        </div>
        <Show when={open() && children.length > 0}>
          <ul>
            <For each={children}>{(c) => <Node item={c} depth={p.depth + 1} />}</For>
          </ul>
        </Show>
      </li>
    );
  };
  // The paper's own bookmarks, else the outline found from its headings (P1.M4.OBJ.4), then
  // jump lists of figures, tables and equations. Refreshed when preparation finishes.
  const [contents, { refetch }] = createResource(() => props.paper.file, (file) => bus("document.contents", { file }).catch(() => null));
  const off = onBusEvent("document.prepared", (e) => {
    if ((e.data as { file?: { hash?: string } }).file?.hash === props.paper.file.hash) void refetch();
  });
  onCleanup(() => void off.then((f) => f()));
  const sections = (): OutlineItem[] => (items()?.length ? (items() as OutlineItem[]) : (contents()?.sections ?? []));
  const Jump = (q: { title: string; list: DocObject[] }): JSX.Element => {
    const [open, setOpen] = createSignal(false);
    return (
      <Show when={q.list.length > 0}>
        <div class="jump">
          <button type="button" class="jump-head" aria-expanded={open()} onClick={() => setOpen(!open())}>
            <Icon name={open() ? "chevron-down" : "chevron-right"} size={14} />
            <span>{q.title}</span>
            <span class="muted small">{q.list.length}</span>
          </button>
          <Show when={open()}>
            <ul class="outline">
              <For each={q.list}>
                {(o) => (
                  <li>
                    <button type="button" class="outline-title jump-item" onClick={() => props.onGoTo(o.rect.page, Math.max(0, o.rect.y0 - 40))}>
                      <strong>{o.label}</strong>
                      <span class="muted small">{(o.text ?? "").replace(o.label ?? "", "").replace(/^[\s:.]+/, "").slice(0, 90)}</span>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>
      </Show>
    );
  };
  return (
    <Show when={items() !== undefined} fallback={<p class="muted">{t("viewer.loading")}</p>}>
      <Show when={sections().length > 0} fallback={<p class="muted">{t("panel.no_outline")}</p>}>
        <Show when={!items()?.length}>
          <p class="muted small">{t("intel.generated_outline")}</p>
        </Show>
        <ul class="outline">
          <For each={sections()}>{(item) => <Node item={item} depth={0} />}</For>
        </ul>
      </Show>
      <Show when={contents()}>
        {(c) => (
          <div class="jumps">
            <Jump title={t("intel.figures")} list={c().figures} />
            <Jump title={t("intel.tables")} list={c().tables} />
            <Jump title={t("intel.equations")} list={c().equations} />
          </div>
        )}
      </Show>
    </Show>
  );
}

function Thumbnails(props: { paper: Paper; pages: number; current: number; onGoTo: (page: number, y: number | null) => void }): JSX.Element {
  let list!: HTMLDivElement;
  const [urls, setUrls] = createSignal<Record<number, string>>({});
  onMount(() => {
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          const page = Number((e.target as HTMLElement).dataset.page);
          io.unobserve(e.target);
          void bus("document.render", { file: props.paper.file, page, width_px: Math.round(120 * (window.devicePixelRatio || 1)) })
            .then((r) => setUrls((u) => ({ ...u, [page]: renderUrl(r.image.hash) })))
            .catch(() => {});
        }
      },
      { root: list, rootMargin: "300px" },
    );
    const watch = (): void => list.querySelectorAll<HTMLElement>("[data-page]").forEach((el) => io.observe(el));
    queueMicrotask(watch);
    onCleanup(() => io.disconnect());
  });
  createEffect(
    on(
      () => props.current,
      (p) => list.querySelector(`[data-page="${p}"]`)?.scrollIntoView({ block: "nearest" }),
    ),
  );
  return (
    <div class="thumbs" ref={list}>
      <For each={Array.from({ length: props.pages }, (_, i) => i + 1)}>
        {(p) => (
          <button type="button" class="thumb" classList={{ active: p === props.current }} data-page={p} onClick={() => props.onGoTo(p, null)}>
            <Show when={urls()[p]} fallback={<div class="thumb-placeholder" />}>
              {(src) => <img src={src()} alt="" draggable={false} />}
            </Show>
            <span>{p}</span>
          </button>
        )}
      </For>
    </div>
  );
}

function Find(props: { paper: Paper | null; openPapers: Paper[]; onHits: (hits: Hit[], active: number | null) => void; focus: number }): JSX.Element {
  let input!: HTMLInputElement;
  const [opts, setOpts] = createSignal<FindOptions>({ query: "", mode: "word_forms", caseSensitive: false, wholeWord: true, allTabs: false });
  const [hits, setHits] = createSignal<Hit[]>([]);
  const [total, setTotal] = createSignal<number | null>(null);
  const [active, setActive] = createSignal<number | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  createEffect(on(() => props.focus, () => queueMicrotask(() => input?.select())));

  const run = async (): Promise<void> => {
    const o = opts();
    const papers = o.allTabs ? props.openPapers : props.paper ? [props.paper] : [];
    if (!o.query.trim() || papers.length === 0) {
      setHits([]);
      setTotal(null);
      props.onHits([], null);
      return;
    }
    try {
      setError(null);
      const r = await bus("document.search", {
        papers: papers.map((p) => p.file.hash),
        terms: [{ text: o.query.trim() }],
        match: { mode: o.mode, case_sensitive: o.caseSensitive, whole_word: o.wholeWord },
        context: { unit: "words", before: 8, after: 8 },
        limits: { max_hits: 500 },
      });
      setHits(r.hits);
      setTotal(r.counts.reduce((a, c) => a + c.total, 0));
      setActive(r.hits.length > 0 ? 0 : null);
      props.onHits(r.hits, r.hits.length > 0 ? 0 : null);
    } catch (e) {
      setError(e instanceof BusError ? e.message : String(e));
    }
  };

  let timer: number | undefined;
  const update = (patch: Partial<FindOptions>): void => {
    setOpts({ ...opts(), ...patch });
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void run(), 200);
  };
  const step = (d: number): void => {
    const n = hits().length;
    if (n === 0) return;
    const next = ((active() ?? -1) + d + n) % n;
    setActive(next);
    props.onHits(hits(), next);
  };
  const paperName = (hash: string): string => {
    const p = props.openPapers.find((x) => x.file.hash === hash);
    return p?.title ?? p?.original_name ?? "";
  };

  return (
    <div class="find">
      <input
        ref={input}
        class="ep-input"
        type="search"
        placeholder={t("find.placeholder")}
        value={opts().query}
        onInput={(e) => update({ query: e.currentTarget.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            step(e.shiftKey ? -1 : 1);
          }
        }}
      />
      <div class="find-options">
        <label class="field">
          {t("find.mode")}
          <select class="ep-select" value={opts().mode} onChange={(e) => update({ mode: e.currentTarget.value as FindOptions["mode"] })}>
            <option value="word_forms">{t("find.mode_word_forms")}</option>
            <option value="exact">{t("find.mode_exact")}</option>
            <option value="fuzzy">{t("find.mode_fuzzy")}</option>
          </select>
        </label>
        <label class="check">
          <input type="checkbox" checked={opts().caseSensitive} onChange={(e) => update({ caseSensitive: e.currentTarget.checked })} />
          {t("find.case")}
        </label>
        <label class="check">
          <input type="checkbox" checked={opts().wholeWord} onChange={(e) => update({ wholeWord: e.currentTarget.checked })} />
          {t("find.whole")}
        </label>
        <label class="check">
          <input type="checkbox" checked={opts().allTabs} onChange={(e) => update({ allTabs: e.currentTarget.checked })} />
          {t("find.all_tabs")}
        </label>
      </div>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
      <Show when={total() !== null}>
        <p class="muted">{total() === 0 ? t("find.none") : t("find.results", { n: total() ?? 0 })}</p>
      </Show>
      <ol class="find-results">
        <For each={hits()}>
          {(h, i) => (
            <li>
              <button
                type="button"
                class="find-hit"
                classList={{ active: active() === i() }}
                onClick={() => {
                  setActive(i());
                  props.onHits(hits(), i());
                }}
              >
                <span class="muted">
                  <Show when={opts().allTabs}>{paperName(h.paper)} · </Show>p. {h.page}
                </span>
                <span>
                  {h.before} <mark>{h.matched}</mark> {h.after}
                </span>
              </button>
            </li>
          )}
        </For>
      </ol>
    </div>
  );
}

function Bookmarks(props: { paper: Paper; current: number; onGoTo: (page: number, y: number | null) => void }): JSX.Element {
  const [list, setList] = createSignal<Bookmark[]>([]);
  const load = async (): Promise<void> => {
    const st = await bus("workspace.get_reading_state", { paper: props.paper.id });
    setList(st.bookmarks ?? []);
  };
  createEffect(on(() => props.paper.id, () => void load()));
  const add = async (): Promise<void> => {
    await bus("workspace.add_bookmark", { name: "", location: { paper: props.paper.id, page: props.current } });
    await load();
  };
  const remove = async (b: Bookmark): Promise<void> => {
    await bus("workspace.remove_bookmark", { id: b.id });
    await load();
  };
  return (
    <div class="bookmarks">
      <Button icon="bookmark-plus" onClick={() => void add()}>{t("panel.add_bookmark")}</Button>
      <Show when={list().length > 0} fallback={<p class="muted">{t("panel.no_bookmarks")}</p>}>
        <ul>
          <For each={list()}>
            {(b) => (
              <li class="bookmark-row">
                <button type="button" class="outline-title" onClick={() => props.onGoTo(b.location.page, b.location.target?.y0 ?? null)}>
                  {b.name}
                </button>
                <IconButton icon="trash" size="sm" class="row-action" label={t("panel.remove_bookmark")} onClick={() => void remove(b)} />
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}
