// The library view (P1.M2.SH.4, folders P1.M4.RSP.14): home with its folders, all papers, recently
// opened, tags; search by title, authors or DOI; sort; several papers at once: select, move by menu
// or by dragging, remove with Undo, and suggested sorting (design language §17.10).

import type { Paper, PaperSort } from "@electropaper/bindings";
import { Button, EmptyState, Icon, IconButton, Segmented, openMenu, showToast, t } from "@electropaper/kit";
import { For, type JSX, Show, createEffect, createMemo, createSignal, onCleanup, onMount, untrack } from "solid-js";

import { bus, renderUrl } from "../host";

import { type FolderGroup, applyFolders, collections, filter, papers, pathOf, project, removePapers, renamePaper, setFilter, setPaperTags, tags, titleOf } from "../library";
import { ask, askOpen, makeDigests, setAskOpen } from "../assistant/ask";
import { canExplain, refreshLanguageModels } from "../assistant/explain";
import { setSettingsOpen } from "../intel/state";
import { AskView } from "./AskView";
import { FolderPath, FolderTiles, FolderTree, dropOn, folderMenu, setDropOn } from "./LibraryFolders";
import { Prompt } from "./Overlays";
import { SortSheet } from "./SortSheet";
import { SummarySheet } from "./SummarySheet";

export interface LibraryViewProps {
  onOpen: (p: Paper) => void;
  /** Open a paper at a place. */
  onOpenAt: (p: Paper, page: number, y: number) => void;
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
  const safe = (f: () => Promise<unknown>): void => void f().catch(props.onError);
  const act = { safe, confirm: (m: string) => props.confirm(m) };

  // ---- selecting several papers ----------------------------------------------------------------
  const [selected, setSelected] = createSignal<ReadonlySet<string>>(new Set());
  let anchor: string | null = null;
  const chosen = createMemo(() => papers().filter((p) => selected().has(p.id)));
  // Only what is shown can be selected: nothing unseen is moved or removed.
  createEffect(() => {
    const here = new Set(papers().map((p) => p.id));
    if ([...untrack(selected)].some((x) => !here.has(x))) setSelected((v) => new Set([...v].filter((x) => here.has(x))));
  });
  const toggle = (id: string): void => {
    setSelected((v) => (v.has(id) ? new Set([...v].filter((x) => x !== id)) : new Set([...v, id])));
    anchor = id;
  };
  const range = (id: string): void => {
    const ids = papers().map((p) => p.id);
    const [i, j] = [ids.indexOf(anchor ?? id), ids.indexOf(id)].sort((x, y) => x - y) as [number, number];
    setSelected((v) => new Set([...v, ...ids.slice(Math.max(0, i), j + 1)]));
  };
  const clear = (): void => void setSelected(new Set<string>());
  /** A click on a paper: opens it, or selects while something is selected. */
  const click = (e: MouseEvent, p: Paper): void => {
    if (dragged) return;
    if (e.shiftKey) range(p.id);
    else if (e.ctrlKey || e.metaKey || selected().size > 0) toggle(p.id);
    else props.onOpen(p);
  };
  const Check = (c: { paper: Paper }): JSX.Element => (
    <input
      type="checkbox"
      class="paper-check"
      checked={selected().has(c.paper.id)}
      aria-label={t("library.select", { title: titleOf(c.paper) })}
      onClick={(e) => {
        e.stopPropagation();
        if (e.shiftKey) {
          e.preventDefault();
          range(c.paper.id);
        } else toggle(c.paper.id);
      }}
    />
  );

  // ---- moving and removing, each with Undo -------------------------------------------------------
  const undoable = (message: string, undo: () => Promise<void>): void => void showToast(message, { label: t("common.undo"), run: () => void undo().catch(props.onError) });
  const move = (ids: string[], to: string | null): void =>
    safe(async () => {
      const undo = await applyFolders([to ? { papers: ids, collection: to } : { papers: ids }]);
      clear();
      undoable(to ? t("library.moved", { n: ids.length, to: pathOf(to) }) : t("library.moved_out", { n: ids.length }), undo);
    });
  const remove = (list: Paper[]): void =>
    safe(async () => {
      const undo = await removePapers(list.map((p) => p.id));
      for (const p of list) props.onRemoved?.(p);
      clear();
      undoable(list.length === 1 ? t("library.removed", { title: titleOf(list[0]) }) : t("library.removed_many", { n: list.length }), undo);
    });
  const [namingFor, setNamingFor] = createSignal<string[] | null>(null);
  const moveMenu = (at: DOMRect, ids: string[]): void => folderMenu(t("library.move_to"), at, (to) => move(ids, to), { top: t("library.no_folder"), make: () => setNamingFor(ids) });
  const intoNew = (name: string): void => {
    const ids = namingFor() ?? [];
    setNamingFor(null);
    safe(async () => {
      const undo = await applyFolders([{ papers: ids, new: { name, parent: filter().collection } }]);
      clear();
      undoable(t("library.moved", { n: ids.length, to: name }), undo);
    });
  };
  const [summaryOf, setSummaryOf] = createSignal<Paper | null>(null);
  /** Summaries of the selected papers, in the background. */
  const summarise = (): void =>
    safe(async () => {
      await refreshLanguageModels();
      if (!canExplain()) return void showToast(t("explain.no_model"), { label: t("explain.open_settings"), run: () => setSettingsOpen(true) });
      const ids = [...selected()];
      await makeDigests({ full: false, papers: ids });
      clear();
      showToast(t("summary.started", { n: ids.length }), { label: t("summary.show"), run: () => setAskOpen(true) });
    });
  const [sorting, setSorting] = createSignal<Paper[] | null>(null);
  const applySorting = (groups: FolderGroup[]): void => {
    setSorting(null);
    safe(async () => {
      const undo = await applyFolders(groups);
      clear();
      undoable(t("sort.done", { n: groups.reduce((a, g) => a + g.papers.length, 0), folders: groups.length }), undo);
    });
  };

  // ---- dragging onto a folder (mouse and pen; a touch scrolls) -----------------------------------
  const [drag, setDrag] = createSignal<{ ids: string[]; x: number; y: number } | null>(null);
  let press: { id: string; x: number; y: number } | null = null;
  let dragged = false;
  const folderAt = (x: number, y: number): string | null => document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-folder]")?.dataset.folder ?? null;
  const onPress = (e: PointerEvent, p: Paper): void => {
    dragged = false;
    if (e.button !== 0 || e.pointerType === "touch" || renaming() || (e.target as HTMLElement).closest("input, select, .row-action, .cover-menu")) return;
    press = { id: p.id, x: e.clientX, y: e.clientY };
  };
  onMount(() => {
    const onMove = (e: PointerEvent): void => {
      if (!press) return;
      if (!drag() && Math.hypot(e.clientX - press.x, e.clientY - press.y) < 8) return;
      dragged = true;
      const ids = selected().has(press.id) ? [...selected()] : [press.id];
      setDrag({ ids, x: e.clientX, y: e.clientY });
      setDropOn(folderAt(e.clientX, e.clientY));
    };
    const onUp = (e: PointerEvent): void => {
      const d = drag();
      press = null;
      setDrag(null);
      setDropOn(null);
      if (!d || e.type === "pointercancel") return;
      const to = folderAt(e.clientX, e.clientY);
      if (to !== null) move(d.ids, to || null);
      // The click that follows a drag does not open the paper.
      window.setTimeout(() => {
        dragged = false;
      }, 0);
    };
    const onKey = (e: KeyboardEvent): void => {
      const el = e.target as HTMLElement;
      if (askOpen()) return;
      if (el.closest("input:not(.paper-check), textarea, select, .ep-dialog, .ep-menu")) return;
      if (e.key === "Escape" && drag()) {
        press = null;
        setDrag(null);
        setDropOn(null);
      } else if (e.key === "Escape" && selected().size > 0) clear();
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelected(new Set(papers().map((p) => p.id)));
      } else if (e.key === "Delete" && selected().size > 0) {
        e.preventDefault();
        e.stopPropagation();
        remove(chosen());
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("keydown", onKey);
    });
  });

  const menu = (at: DOMRect, p: Paper): void =>
    openMenu(
      titleOf(p),
      [
        { label: t("library.open"), icon: "file-plus", run: () => props.onOpen(p) },
        { label: t("summary.title"), icon: "list-tree", run: () => setSummaryOf(p) },
        { label: t("library.rename"), icon: "pen-line", shortcut: "F2", run: () => setRenaming(p.id) },
        { label: t("library.move_to"), icon: "folder", run: () => moveMenu(at, [p.id]) },
        { separator: true },
        { label: t("library.remove"), icon: "trash", shortcut: "Delete", run: () => remove([p]) },
      ],
      at,
    );
  const keys = (e: KeyboardEvent, p: Paper): void => {
    if (renaming() || editingTags()) return;
    if (e.key === "F2") {
      e.preventDefault();
      setRenaming(p.id);
    } else if (e.key === "Delete" && selected().size === 0) {
      e.preventDefault();
      remove([p]);
    }
  };
  const [editingTags, setEditingTags] = createSignal<string | null>(null);
  const recentSelected = (): boolean => filter().sort === "opened" && !filter().collection && !filter().tag && !filter().unfiled;
  /** A filter of the list; it leaves asking. */
  const show = (f: Parameters<typeof setFilter>[0]): void => {
    setAskOpen(false);
    setFilter(f);
  };
  const homeSelected = (): boolean => filter().unfiled && !filter().collection && !filter().tag;

  return (
    <div class="library-view">
      <nav class="library-nav">
        <h2>{project()?.name ?? t("library.title")}</h2>
        <button type="button" class="nav-item nav-ask" classList={{ active: askOpen() }} title={`${t("ask.title")} (Ctrl+Shift+L)`} onClick={() => setAskOpen(true)}>
          <span class="nav-name">{t("ask.title")}</span>
          <Icon name="sparkles" size={16} />
        </button>
        <button type="button" class="nav-item" data-folder="" classList={{ active: !askOpen() && homeSelected(), drop: dropOn() === "" }} onClick={() => show({ collection: null, unfiled: true, tag: null })}>
          {t("library.home")}
        </button>
        <button type="button" class="nav-item" classList={{ active: !askOpen() && !filter().collection && !filter().tag && !filter().unfiled && !recentSelected() }} onClick={() => show({ collection: null, unfiled: false, tag: null, sort: "added" })}>
          {t("library.all")}
        </button>
        <button type="button" class="nav-item" classList={{ active: !askOpen() && recentSelected() }} onClick={() => show({ collection: null, unfiled: false, tag: null, sort: "opened" })}>
          {t("library.recent")}
        </button>
        <h3>{t("library.folders")}</h3>
        <FolderTree {...act} />
        <Show when={tags().length > 0}>
          <h3>{t("library.tags")}</h3>
          <div class="tag-cloud">
            <For each={tags()}>
              {(tg) => (
                <button type="button" class="tag" classList={{ active: !askOpen() && filter().tag === tg.tag }} onClick={() => show({ tag: filter().tag === tg.tag ? null : tg.tag, collection: null, unfiled: false })}>
                  {tg.tag} <span class="muted">{tg.papers}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </nav>
      <Show when={askOpen()}>
        <AskView onOpenAt={props.onOpenAt} onError={props.onError} />
      </Show>
      <section class="library-main" classList={{ away: askOpen() }}>
        <Show when={selected().size > 0}>
          <div class="library-selection" role="toolbar" aria-label={t("library.selection")}>
            <strong>{t("library.selected", { n: selected().size })}</strong>
            <Button icon="folder" onClick={(e) => moveMenu((e.currentTarget as HTMLElement).getBoundingClientRect(), [...selected()])}>
              {t("library.move_to")}
            </Button>
            <Button icon="sparkles" onClick={() => setSorting(chosen())}>
              {t("sort.action")}
            </Button>
            <Button icon="list-tree" onClick={summarise}>
              {t("summary.make_short")}
            </Button>
            <Button icon="trash" onClick={() => remove(chosen())}>
              {t("library.remove_short")}
            </Button>
            <Button variant="quiet" onClick={() => setSelected(new Set(papers().map((p) => p.id)))}>
              {t("library.select_all")}
            </Button>
            <Button variant="quiet" icon="x" onClick={clear}>
              {t("library.clear")}
            </Button>
          </div>
        </Show>
        <div class="library-tools" classList={{ away: selected().size > 0 }}>
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
        <FolderPath />
        <FolderTiles {...act} />
        <Show
          when={papers().length > 0}
          fallback={
            <EmptyState icon="library" text={filter().query || filter().tag ? t("library.no_match") : filter().collection ? t("library.empty_folder") : collections().length > 0 && filter().unfiled ? t("library.all_filed") : t("library.empty")} />
          }
        >
          <Show
            when={view() === "covers"}
            fallback={
              <ul class="paper-list">
                <For each={papers()}>
                  {(p) => (
                    <li class="paper-item" classList={{ selected: selected().has(p.id) }} data-paper={p.id} tabIndex={-1} onKeyDown={(e) => keys(e, p)} onPointerDown={(e) => onPress(e, p)}>
                      <Check paper={p} />
                      <Show
                        when={renaming() === p.id}
                        fallback={
                          <button type="button" class="paper-open" onClick={(e) => click(e, p)}>
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
                  <li class="paper-cover" classList={{ selected: selected().has(p.id) }} data-paper={p.id} tabIndex={-1} onKeyDown={(e) => keys(e, p)} onPointerDown={(e) => onPress(e, p)}>
                    <Check paper={p} />
                    <button type="button" class="cover-open" onClick={(e) => click(e, p)} aria-label={p.title ?? p.original_name}>
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
      <Show when={drag()}>
        {(d) => (
          <div class="drag-ghost" aria-hidden="true" style={{ left: `${d().x + 12}px`, top: `${d().y + 12}px` }}>
            {d().ids.length === 1 ? titleOf(papers().find((p) => p.id === d().ids[0])) : t("library.selected", { n: d().ids.length })}
          </div>
        )}
      </Show>
      <Show when={namingFor()}>
        <Prompt message={t("library.new_folder")} placeholder={t("library.folder_name")} action={t("library.make_and_move")} onAnswer={(name) => (name ? intoNew(name) : setNamingFor(null))} />
      </Show>
      <Show when={summaryOf()} keyed>
        {(p) => (
          <SummarySheet
            paper={p}
            onClose={() => setSummaryOf(null)}
            onError={props.onError}
            onOpenAt={(page, y) => props.onOpenAt(p, page, y)}
            onAsk={(q) => {
              setSummaryOf(null);
              setAskOpen(true);
              void ask(q);
            }}
          />
        )}
      </Show>
      <Show when={sorting()}>{(list) => <SortSheet papers={list()} parent={filter().collection} onApply={applySorting} onClose={() => setSorting(null)} />}</Show>
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
