// The annotations side panel (P1.M3.UI.7): every mark and comment of the paper, filtered by
// colour, category, page or author, searchable; a click goes to the mark and opens its card.

import type { Annotation, Category, Paper } from "@electropaper/bindings";
import { t } from "@electropaper/kit";
import { For, type JSX, Show, createMemo, createSignal } from "solid-js";

import { categoryLabel, kindLabel } from "./Editor";
import { CATEGORIES, annotationsOf, repliesOf, setPicked } from "./state";

export function NotesPanel(props: { paper: Paper; current: number; onGoTo: (page: number, y: number | null) => void }): JSX.Element {
  const [query, setQuery] = createSignal("");
  const [colors, setColors] = createSignal<Set<string>>(new Set());
  const [cat, setCat] = createSignal<Category | "">("");
  const [author, setAuthor] = createSignal("");
  const [thisPage, setThisPage] = createSignal(false);

  const all = createMemo(() => (annotationsOf(props.paper.id)?.annotations ?? []).filter((a) => !a.parent));
  const usedColors = createMemo(() => [...new Set(all().map((a) => a.color))]);
  const authors = createMemo(() => [...new Set((annotationsOf(props.paper.id)?.annotations ?? []).map((a) => a.author).filter((x): x is string => !!x))].sort());

  const list = createMemo(() => {
    const q = query().trim().toLowerCase();
    const cs = colors();
    return all().filter((a) => {
      if (cs.size > 0 && !cs.has(a.color)) return false;
      if (cat() && a.category !== cat()) return false;
      if (author() && a.author !== author()) return false;
      if (thisPage() && a.page !== props.current) return false;
      if (!q) return true;
      const replies = repliesOf(props.paper.id, a.id).map((r) => r.body ?? "");
      return [a.body ?? "", a.text ?? "", ...replies].some((s) => s.toLowerCase().includes(q));
    });
  });

  const toggleColor = (c: string): void => {
    const next = new Set(colors());
    if (next.has(c)) next.delete(c);
    else next.add(c);
    setColors(next);
  };

  const open = (a: Annotation): void => {
    props.onGoTo(a.page, Math.max(0, a.rect.y0 - 60));
    setPicked({ paper: props.paper.id, id: a.id, edit: true });
  };

  return (
    <div class="notes-panel">
      <input class="ep-input" type="search" placeholder={t("annot.search")} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      <div class="filters">
        <div class="swatches">
          <For each={usedColors()}>
            {(c) => <button type="button" class="swatch" aria-pressed={colors().has(c)} aria-label={c} style={{ background: c }} onClick={() => toggleColor(c)} />}
          </For>
        </div>
        <select class="ep-select" aria-label={t("annot.category")} value={cat()} onChange={(e) => setCat(e.currentTarget.value as Category | "")}>
          <option value="">{t("annot.all_categories")}</option>
          <For each={CATEGORIES}>{(c) => <option value={c}>{categoryLabel(c)}</option>}</For>
        </select>
        <Show when={authors().length > 1}>
          <select class="ep-select" aria-label={t("annot.author")} value={author()} onChange={(e) => setAuthor(e.currentTarget.value)}>
            <option value="">{t("annot.all_authors")}</option>
            <For each={authors()}>{(a) => <option value={a}>{a}</option>}</For>
          </select>
        </Show>
        <label class="check">
          <input type="checkbox" checked={thisPage()} onChange={(e) => setThisPage(e.currentTarget.checked)} />
          {t("annot.this_page")}
        </label>
      </div>
      <p class="muted small">{t("annot.count", { n: list().length, total: all().length })}</p>
      <Show when={all().length > 0} fallback={<p class="muted">{t("annot.none")}</p>}>
        <ul class="notes-list">
          <For each={list()}>
            {(a) => {
              const replies = (): number => repliesOf(props.paper.id, a.id).length;
              return (
                <li>
                  <button type="button" class="note-row" style={{ "border-inline-start-color": a.color }} onClick={() => open(a)}>
                    <span class="muted small">
                      {t("viewer.page_box")} {a.page} · {kindLabel(a.kind)}
                      <Show when={a.category}>{(c) => <> · {categoryLabel(c())}</>}</Show>
                      <Show when={a.resolved}> · {t("annot.resolved")}</Show>
                    </span>
                    <Show when={a.text}>
                      <span class="quote">{a.text}</span>
                    </Show>
                    <Show when={a.body}>
                      <span class="body">{a.body}</span>
                    </Show>
                    <Show when={replies() > 0}>
                      <span class="muted small">{t("annot.replies", { n: replies() })}</span>
                    </Show>
                  </button>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </div>
  );
}
