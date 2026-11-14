// The library view (P1.M2.SH.4): all papers, recently opened, collections and tags; search by
// title, authors or DOI; sort; per-paper tags and collections.

import type { Paper, PaperSort } from "@electropaper/bindings";
import { Button, EmptyState, IconButton, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal } from "solid-js";

import { addToCollection, collections, createCollection, deleteCollection, filter, papers, project, setFilter, setPaperTags, tags } from "../library";

export interface LibraryViewProps {
  onOpen: (p: Paper) => void;
  onAdd: () => void;
  confirm: (message: string) => Promise<boolean>;
  onError: (e: unknown) => void;
}

export function LibraryView(props: LibraryViewProps): JSX.Element {
  const [naming, setNaming] = createSignal(false);
  const [editingTags, setEditingTags] = createSignal<string | null>(null);
  const safe = (f: () => Promise<unknown>): void => void f().catch(props.onError);
  const recentSelected = (): boolean => filter().sort === "opened" && !filter().collection && !filter().tag;

  return (
    <div class="library-view">
      <nav class="library-nav">
        <h2>{project()?.name ?? t("library.title")}</h2>
        <button type="button" class="nav-item" classList={{ active: !filter().collection && !filter().tag && !recentSelected() }} onClick={() => setFilter({ collection: null, tag: null, sort: "added" })}>
          {t("library.all")}
        </button>
        <button type="button" class="nav-item" classList={{ active: recentSelected() }} onClick={() => setFilter({ collection: null, tag: null, sort: "opened" })}>
          {t("library.recent")}
        </button>
        <h3>{t("library.collections")}</h3>
        <For each={collections()}>
          {(c) => (
            <div class="nav-row">
              <button type="button" class="nav-item" classList={{ active: filter().collection === c.id }} onClick={() => setFilter({ collection: c.id, tag: null })}>
                {c.name} <span class="muted">{c.papers}</span>
              </button>
              <IconButton icon="trash" size="sm" class="row-action" label={t("library.delete_collection")} onClick={() => safe(() => deleteCollection(c, props.confirm))} />
            </div>
          )}
        </For>
        <Show
          when={naming()}
          fallback={
            <Button variant="quiet" icon="plus" class="nav-new" onClick={() => setNaming(true)}>
              {t("library.new_collection")}
            </Button>
          }
        >
          <input
            class="ep-input"
            placeholder={t("library.collection_name")}
            ref={(el) => queueMicrotask(() => el.focus())}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.currentTarget.value.trim()) {
                const name = e.currentTarget.value;
                setNaming(false);
                safe(() => createCollection(name));
              }
              if (e.key === "Escape") setNaming(false);
            }}
            onBlur={() => setNaming(false)}
          />
        </Show>
        <Show when={tags().length > 0}>
          <h3>{t("library.tags")}</h3>
          <div class="tag-cloud">
            <For each={tags()}>
              {(tg) => (
                <button type="button" class="tag" classList={{ active: filter().tag === tg.tag }} onClick={() => setFilter({ tag: filter().tag === tg.tag ? null : tg.tag, collection: null })}>
                  {tg.tag} <span class="muted">{tg.papers}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </nav>
      <section class="library-main">
        <div class="library-tools">
          <input class="ep-input grow" type="search" aria-label={t("library.search")} placeholder={t("library.search")} value={filter().query} onInput={(e) => setFilter({ query: e.currentTarget.value })} />
          <label class="field">
            {t("library.sort")}
            <select class="ep-select" value={filter().sort} onChange={(e) => setFilter({ sort: e.currentTarget.value as PaperSort })}>
              <option value="added">{t("library.sort_added")}</option>
              <option value="opened">{t("library.sort_opened")}</option>
              <option value="title">{t("library.sort_title")}</option>
            </select>
          </label>
          <Button variant="primary" icon="file-plus" onClick={props.onAdd}>
            {t("library.add")}
          </Button>
        </div>
        <Show when={papers().length > 0} fallback={<EmptyState icon="library" text={filter().query || filter().collection || filter().tag ? t("library.no_match") : t("library.empty")} />}>
          <ul class="paper-list">
            <For each={papers()}>
              {(p) => (
                <li class="paper-item">
                  <button type="button" class="paper-open" onDblClick={() => props.onOpen(p)} onClick={() => props.onOpen(p)}>
                    <span class="paper-title">{p.title ?? p.original_name ?? t("library.untitled")}</span>
                    <span class="muted">
                      {(p.authors ?? []).slice(0, 4).join(", ")}
                      {p.pages ? ` · ${t("library.pages", { n: p.pages })}` : ""}
                      {p.doi ? ` · ${p.doi}` : ""}
                    </span>
                  </button>
                  <div class="paper-meta">
                    <Show
                      when={editingTags() === p.id}
                      fallback={
                        <button type="button" class="tags-line" onClick={() => setEditingTags(p.id)} title={t("library.edit_tags")}>
                          <For each={p.tags ?? []}>{(tg) => <span class="tag small">{tg}</span>}</For>
                          <span class="muted small">{(p.tags ?? []).length ? "" : t("library.add_tags")}</span>
                        </button>
                      }
                    >
                      <input
                        class="ep-input"
                        value={(p.tags ?? []).join(", ")}
                        placeholder={t("library.edit_tags")}
                        ref={(el) => queueMicrotask(() => el.focus())}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            const v = e.currentTarget.value;
                            setEditingTags(null);
                            safe(() => setPaperTags(p.id, v));
                          }
                          if (e.key === "Escape") setEditingTags(null);
                        }}
                        onBlur={() => setEditingTags(null)}
                      />
                    </Show>
                    <Show when={collections().length > 0}>
                      <select
                        class="ep-select small"
                        value=""
                        aria-label={t("library.add_to")}
                        onChange={(e) => {
                          const c = e.currentTarget.value;
                          e.currentTarget.value = "";
                          if (c) safe(() => addToCollection(c, p.id));
                        }}
                      >
                        <option value="">{t("library.add_to")}…</option>
                        <For each={collections()}>{(c) => <option value={c.id}>{c.name}</option>}</For>
                      </select>
                    </Show>
                  </div>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </section>
    </div>
  );
}
