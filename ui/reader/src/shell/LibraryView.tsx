// The library view (P1.M2.SH.4): all papers, recently opened, collections and tags; search by
// title, authors or DOI; sort; per-paper tags and collections.

import type { Paper, PaperSort } from "@electropaper/bindings";
import { Button, EmptyState, IconButton, Segmented, openMenu, showToast, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";

import { bus, renderUrl } from "../host";

import { addToCollection, collections, createCollection, deleteCollection, filter, papers, project, removePaper, renamePaper, setFilter, setPaperTags, tags } from "../library";

export interface LibraryViewProps {
  onOpen: (p: Paper) => void;
  onAdd: () => void;
  confirm: (message: string) => Promise<boolean>;
  onError: (e: unknown) => void;
  /** A paper left the library (its open tabs close). */
  onRemoved?: (p: Paper) => void;
}

export function LibraryView(props: LibraryViewProps): JSX.Element {
  // List or covers, remembered (design language §17.10).
  const readView = (): "list" | "covers" => {
    try {
      return globalThis.localStorage?.getItem("ep.library-view") === "covers" ? "covers" : "list";
    } catch {
      return "list";
    }
  };
  const [view, setViewSignal] = createSignal<"list" | "covers">(readView());
  const setView = (v: "list" | "covers"): void => {
    setViewSignal(v);
    try {
      globalThis.localStorage?.setItem("ep.library-view", v);
    } catch {
      // A convenience.
    }
  };
  const [renaming, setRenaming] = createSignal<string | null>(null);
  // Remove at once, with Undo (no confirmation: §9.8).
  const remove = async (p: Paper): Promise<void> => {
    try {
      const undo = await removePaper(p.id);
      props.onRemoved?.(p);
      showToast(t("library.removed", { title: p.title ?? p.original_name }), { label: t("common.undo"), run: () => void undo().catch(props.onError) });
    } catch (e) {
      props.onError(e);
    }
  };
  const menu = (at: DOMRect, p: Paper): void =>
    openMenu(
      p.title ?? p.original_name,
      [
        { label: t("library.open"), icon: "file-plus", run: () => props.onOpen(p) },
        { label: t("library.rename"), icon: "pen-line", shortcut: "F2", run: () => setRenaming(p.id) },
        { separator: true },
        { label: t("library.remove"), icon: "trash", shortcut: "Delete", run: () => void remove(p) },
      ],
      at,
    );
  const keys = (e: KeyboardEvent, p: Paper): void => {
    if (renaming() || editingTags()) return;
    if (e.key === "F2") {
      e.preventDefault();
      setRenaming(p.id);
    } else if (e.key === "Delete") {
      e.preventDefault();
      void remove(p);
    }
  };
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
          <Segmented
            label={t("library.view")}
            value={view()}
            options={[
              { value: "list", label: t("library.view_list"), icon: "list-checks" },
              { value: "covers", label: t("library.view_covers"), icon: "layout-grid" },
            ]}
            onChange={setView}
          />
          <Button variant="primary" icon="file-plus" onClick={props.onAdd}>
            {t("library.add")}
          </Button>
        </div>
        <Show when={papers().length > 0} fallback={<EmptyState icon="library" text={filter().query || filter().collection || filter().tag ? t("library.no_match") : t("library.empty")} />}>
          <Show
            when={view() === "covers"}
            fallback={
              <ul class="paper-list">
                <For each={papers()}>
                  {(p) => (
                    <li class="paper-item" tabIndex={-1} onKeyDown={(e) => keys(e, p)}>
                      <Show
                        when={renaming() === p.id}
                        fallback={
                          <button type="button" class="paper-open" onClick={() => props.onOpen(p)}>
                            <span class="paper-title">{p.title ?? p.original_name ?? t("library.untitled")}</span>
                            <span class="muted">
                              {(p.authors ?? []).slice(0, 4).join(", ")}
                              {p.pages ? ` · ${t("library.pages", { n: p.pages })}` : ""}
                              {p.doi ? ` · ${p.doi}` : ""}
                            </span>
                          </button>
                        }
                      >
                        <RenameField paper={p} onDone={() => setRenaming(null)} onError={props.onError} />
                      </Show>
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
                        <IconButton icon="ellipsis" size="sm" class="row-action" label={t("library.paper_menu")} onClick={(e) => menu(e.currentTarget.getBoundingClientRect(), p)} />
                      </div>
                    </li>
                  )}
                </For>
              </ul>
            }
          >
            <ul class="paper-covers">
              <For each={papers()}>
                {(p) => (
                  <li class="paper-cover" tabIndex={-1} onKeyDown={(e) => keys(e, p)}>
                    <button type="button" class="cover-open" onClick={() => props.onOpen(p)} aria-label={p.title ?? p.original_name}>
                      <Cover paper={p} />
                    </button>
                    <div class="cover-text">
                      <Show
                        when={renaming() === p.id}
                        fallback={
                          <span class="cover-title" title={p.title ?? p.original_name}>
                            {p.title ?? p.original_name ?? t("library.untitled")}
                          </span>
                        }
                      >
                        <RenameField paper={p} onDone={() => setRenaming(null)} onError={props.onError} />
                      </Show>
                      <span class="muted small cover-authors">{(p.authors ?? []).slice(0, 3).join(", ") || (p.pages ? t("library.pages", { n: p.pages }) : "")}</span>
                    </div>
                    <IconButton icon="ellipsis" size="sm" class="cover-menu" label={t("library.paper_menu")} onClick={(e) => menu(e.currentTarget.getBoundingClientRect(), p)} />
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>
      </section>
    </div>
  );
}

/** Rename in place: Enter saves, Escape cancels (design language §17.10). */
function RenameField(props: { paper: Paper; onDone: () => void; onError: (e: unknown) => void }): JSX.Element {
  let done = false;
  const save = (v: string): void => {
    if (done) return;
    done = true;
    props.onDone();
    if (v.trim() && v.trim() !== (props.paper.title ?? props.paper.original_name)) void renamePaper(props.paper.id, v).catch(props.onError);
  };
  return (
    <input
      class="ep-input rename-field"
      aria-label={t("library.rename")}
      value={props.paper.title ?? props.paper.original_name ?? ""}
      ref={(el) =>
        queueMicrotask(() => {
          el.focus();
          el.select();
        })
      }
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") save(e.currentTarget.value);
        if (e.key === "Escape") {
          done = true;
          props.onDone();
        }
      }}
      onBlur={(e) => save(e.currentTarget.value)}
    />
  );
}

/** The first page as a cover, drawn when it comes into view. */
function Cover(props: { paper: Paper }): JSX.Element {
  let box!: HTMLSpanElement;
  const [url, setUrl] = createSignal<string | null>(null);
  onMount(() => {
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((x) => x.isIntersecting)) return;
        io.disconnect();
        void bus("document.render", { file: props.paper.file, page: 1, width_px: Math.round(220 * (window.devicePixelRatio || 1)), background: true })
          .then((r) => setUrl(renderUrl(r.image.hash)))
          .catch(() => {});
      },
      { rootMargin: "300px" },
    );
    io.observe(box);
    onCleanup(() => io.disconnect());
  });
  return (
    <span class="cover-page" ref={box}>
      <Show when={url()} fallback={<span class="cover-wait" />}>
        {(u) => <img src={u()} alt="" draggable={false} />}
      </Show>
    </span>
  );
}
