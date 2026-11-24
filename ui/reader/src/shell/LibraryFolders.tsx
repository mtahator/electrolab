// Folders of the library (design language §17.10): the tree at the side, the tiles of the folders in
// the one shown, the way back, and the menu to choose where papers go.

import type { Collection } from "@electropaper/bindings";
import { Button, Icon, IconButton, type MenuItem, openMenu, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal } from "solid-js";

import { childrenOf, collections, createCollection, deleteCollection, filter, moveCollection, pathOf, renameCollection, setFilter } from "../library";

/** Where a new folder is being named (null: nowhere; "": at the top), and which one is renamed. */
const [naming, setNaming] = createSignal<string | null>(null);
const [renaming, setRenaming] = createSignal<string | null>(null);
const [closed, setClosed] = createSignal<ReadonlySet<string>>(new Set());
/** The folder a drag is over. */
export const [dropOn, setDropOn] = createSignal<string | null>(null);

export const newFolder = (parent: string | null): void => {
  if (parent) setClosed((s) => new Set([...s].filter((x) => x !== parent)));
  setNaming(parent ?? "");
};

interface Act {
  safe: (f: () => Promise<unknown>) => void;
  confirm: (message: string) => Promise<boolean>;
}

/** Every folder as a menu, indented; the ones in `without` left out. */
export function folderMenu(title: string, at: DOMRect, choose: (id: string | null) => void, opts: { top: string; without?: string; make?: () => void }): void {
  const items: MenuItem[] = [];
  const add = (parent: string | null, depth: number): void => {
    for (const c of childrenOf(parent)) {
      if (c.id === opts.without) continue;
      items.push({ label: `${" ".repeat(depth)}${c.name}`, icon: "folder", run: () => choose(c.id) });
      add(c.id, depth + 1);
    }
  };
  add(null, 0);
  if (items.length > 0) items.push({ separator: true });
  if (opts.make) items.push({ label: t("library.new_folder"), icon: "plus", run: opts.make });
  items.push({ label: opts.top, icon: "library", run: () => choose(null) });
  openMenu(title, items, at);
}

function menuOf(c: Collection, at: DOMRect, act: Act): void {
  openMenu(
    c.name,
    [
      { label: t("library.new_folder_in"), icon: "plus", run: () => newFolder(c.id) },
      { label: t("library.rename"), icon: "pen-line", run: () => setRenaming(c.id) },
      { label: t("library.move_folder"), icon: "folder", run: () => folderMenu(t("library.move_folder"), at, (to) => act.safe(() => moveCollection(c.id, to)), { top: t("library.top"), without: c.id }) },
      { separator: true },
      { label: t("library.delete_folder"), icon: "trash", run: () => act.safe(() => deleteCollection(c, act.confirm)) },
    ],
    at,
  );
}

/** A name being typed: Enter keeps it, Esc does not. */
function NameField(props: { value: string; onName: (name: string) => void; onDone: () => void }): JSX.Element {
  let done = false;
  const finish = (name: string | null): void => {
    if (done) return;
    done = true;
    props.onDone();
    if (name?.trim()) props.onName(name.trim());
  };
  return (
    <input
      class="ep-input"
      aria-label={t("library.folder_name")}
      placeholder={t("library.folder_name")}
      value={props.value}
      ref={(el) => queueMicrotask(() => (el.focus(), el.select()))}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(e.currentTarget.value);
        if (e.key === "Escape") finish(null);
      }}
      onBlur={(e) => finish(e.currentTarget.value)}
    />
  );
}

export function FolderTree(props: Act): JSX.Element {
  const Level = (p: { parent: string | null }): JSX.Element => (
    <ul class="folder-tree">
      <For each={childrenOf(p.parent)}>
        {(c) => (
          <li>
            <div class="nav-row" classList={{ drop: dropOn() === c.id }} data-folder={c.id}>
              <Show when={childrenOf(c.id).length > 0} fallback={<span class="folder-gap" />}>
                <IconButton
                  icon={closed().has(c.id) ? "chevron-right" : "chevron-down"}
                  size="sm"
                  label={closed().has(c.id) ? t("library.expand") : t("library.collapse")}
                  onClick={() => setClosed((s) => (s.has(c.id) ? new Set([...s].filter((x) => x !== c.id)) : new Set([...s, c.id])))}
                />
              </Show>
              <Show
                when={renaming() === c.id}
                fallback={
                  <button type="button" class="nav-item" classList={{ active: filter().collection === c.id }} onClick={() => setFilter({ collection: c.id, unfiled: false, tag: null })}>
                    <Icon name="folder" size={16} />
                    <span class="nav-name">{c.name}</span>
                    <span class="muted">{c.papers}</span>
                  </button>
                }
              >
                <NameField value={c.name} onName={(n) => props.safe(() => renameCollection(c.id, n))} onDone={() => setRenaming(null)} />
              </Show>
              <IconButton icon="ellipsis" size="sm" class="row-action" label={t("library.folder_menu", { name: c.name })} onClick={(e) => menuOf(c, e.currentTarget.getBoundingClientRect(), props)} />
            </div>
            <Show when={naming() === c.id}>
              <NameField value="" onName={(n) => props.safe(() => createCollection(n, c.id))} onDone={() => setNaming(null)} />
            </Show>
            <Show when={!closed().has(c.id) && childrenOf(c.id).length > 0}>
              <Level parent={c.id} />
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
  return (
    <>
      <Level parent={null} />
      <Show
        when={naming() === ""}
        fallback={
          <Button variant="quiet" icon="plus" class="nav-new" onClick={() => newFolder(null)}>
            {t("library.new_folder")}
          </Button>
        }
      >
        <NameField value="" onName={(n) => props.safe(() => createCollection(n, null))} onDone={() => setNaming(null)} />
      </Show>
    </>
  );
}

/** The way back, above what a folder shows. */
export function FolderPath(): JSX.Element {
  const up = (): Collection[] => {
    const out: Collection[] = [];
    let at = filter().collection;
    while (at && out.length < 20) {
      const c = collections().find((x) => x.id === at);
      if (!c) break;
      out.unshift(c);
      at = c.parent ?? null;
    }
    return out;
  };
  return (
    <Show when={filter().collection}>
      <nav class="folder-path" aria-label={t("library.path")}>
        <button type="button" class="link-button" onClick={() => setFilter({ collection: null, unfiled: true, tag: null })}>
          {t("library.home")}
        </button>
        <For each={up()}>
          {(c) => (
            <>
              <Icon name="chevron-right" size={16} />
              <button type="button" class="link-button" aria-current={c.id === filter().collection ? "page" : undefined} onClick={() => setFilter({ collection: c.id, unfiled: false })}>
                {c.name}
              </button>
            </>
          )}
        </For>
      </nav>
    </Show>
  );
}

/** Tiles of the folders in the one shown (the top ones at home). */
export function FolderTiles(props: Act): JSX.Element {
  const here = (): Collection[] => (filter().query || filter().tag || (!filter().collection && !filter().unfiled) ? [] : childrenOf(filter().collection));
  return (
    <Show when={here().length > 0}>
      <ul class="folder-tiles">
        <For each={here()}>
          {(c) => (
            <li class="folder-tile" classList={{ drop: dropOn() === c.id }} data-folder={c.id}>
              <button type="button" class="folder-open" onClick={() => setFilter({ collection: c.id, unfiled: false, tag: null })} title={pathOf(c.id)}>
                <Icon name="folder" size={20} />
                <span class="nav-name">{c.name}</span>
                <span class="muted">{c.papers}</span>
              </button>
              <IconButton icon="ellipsis" size="sm" class="row-action" label={t("library.folder_menu", { name: c.name })} onClick={(e) => menuOf(c, e.currentTarget.getBoundingClientRect(), props)} />
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
}
