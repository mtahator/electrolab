// Library state and the import flow (the host chains two engines, Draft 2.2 §6.3):
// workspace.add_file → document.metadata → workspace.set_paper_metadata.

import type { Collection, Paper, PaperSort, ProjectInfo, TagCount } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus } from "./host";

export interface Filter {
  query: string;
  collection: string | null;
  /** Home: only the papers in no folder. */
  unfiled: boolean;
  tag: string | null;
  sort: PaperSort;
}

const [project, setProject] = createSignal<ProjectInfo | null>(null);
const [papers, setPapers] = createSignal<Paper[]>([]);
const [collections, setCollections] = createSignal<Collection[]>([]);
const [tags, setTags] = createSignal<TagCount[]>([]);
const [filter, setFilterSignal] = createSignal<Filter>({ query: "", collection: null, unfiled: true, tag: null, sort: "added" });

export { collections, filter, papers, project, tags };

export async function openDefaultLibrary(): Promise<void> {
  setProject(await bus("workspace.open_default_library", null));
  await refresh();
}

/** Reload papers (with the current filter), collections and tags. */
/** Take a paper out of the library; returns a function that brings it back (Undo). */
export async function removePaper(id: string): Promise<() => Promise<void>> {
  await bus("workspace.remove_paper", { id });
  await refresh();
  return async () => {
    await bus("workspace.restore_paper", { id });
    await refresh();
  };
}

/** Rename a paper (its title in the library and tabs; the file name stays). */
export async function renamePaper(id: string, title: string): Promise<void> {
  const v = title.trim();
  if (!v) return;
  await bus("workspace.set_paper_metadata", { id, title: v, source: "user" });
  await refresh();
}

export async function refresh(): Promise<void> {
  const f = filter();
  const [p, c, t] = await Promise.all([
    bus("workspace.list_papers", { limit: 1000, offset: 0, query: f.query || null, collection: f.collection, unfiled: f.unfiled && !f.query && !f.tag && !f.collection, tag: f.tag, sort: f.sort }),
    bus("workspace.list_collections", null),
    bus("workspace.list_tags", null),
  ]);
  setPapers(p);
  setCollections(c);
  setTags(t);
}

let timer: number | undefined;
/** Change the filter; the list follows a moment later. */
export function setFilter(patch: Partial<Filter>): void {
  setFilterSignal({ ...filter(), ...patch });
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void refresh(), 150);
}

/** Add one PDF and record its metadata. Returns the paper. */
export async function importPdf(path: string, sourceUrl: string | null = null): Promise<Paper> {
  const added = await bus("workspace.add_file", { path, source_url: sourceUrl });
  if (added.already_present && added.paper.metadata_source) return added.paper;
  const meta = await bus("document.metadata", { file: added.paper.file });
  return bus("workspace.set_paper_metadata", {
    id: added.paper.id,
    title: meta.title ?? null,
    authors: (meta.authors ?? []).length > 0 ? meta.authors : null,
    doi: meta.doi ?? null,
    pages: meta.pages,
    source: "pdf-info",
  });
}

/** What to call a paper. */
export const titleOf = (p: Paper | undefined): string => p?.title ?? p?.original_name ?? "";

/** Folders directly in a folder (null: at the top), by name. */
export const childrenOf = (parent: string | null): Collection[] => collections().filter((c) => (c.parent ?? null) === parent);

/** "Energy / Wind" */
export function pathOf(id: string): string {
  const names: string[] = [];
  let at: string | null = id;
  while (at && names.length < 20) {
    const c = collections().find((x) => x.id === at);
    if (!c) break;
    names.unshift(c.name);
    at = c.parent ?? null;
  }
  return names.join(" / ");
}

export async function createCollection(name: string, parent: string | null = null): Promise<void> {
  await bus("workspace.create_collection", { name, parent });
  await refresh();
}

export async function renameCollection(id: string, name: string): Promise<void> {
  await bus("workspace.rename_collection", { id, name });
  await refresh();
}

export async function moveCollection(id: string, parent: string | null): Promise<void> {
  await bus("workspace.move_collection", { id, parent });
  await refresh();
}

/** Papers that go together: into a folder, a new folder, or out of all folders. */
export interface FolderGroup {
  papers: string[];
  collection?: string;
  new?: { name: string; parent: string | null };
}

/** Apply groups, all or nothing; returns the way back. */
export async function applyFolders(groups: FolderGroup[]): Promise<() => Promise<void>> {
  const done = await bus("workspace.apply_folders", { groups });
  await refresh();
  return async () => {
    await bus("workspace.undo_folders", done);
    await refresh();
  };
}

/** Take several papers out of the library; returns the way back. */
export async function removePapers(ids: string[]): Promise<() => Promise<void>> {
  await bus("workspace.remove_papers", { ids });
  await refresh();
  return async () => {
    await bus("workspace.restore_papers", { ids });
    await refresh();
  };
}

/** Delete a collection after the user confirms (the policy asks for approval). */
export async function deleteCollection(c: Collection, confirm: (message: string) => Promise<boolean>): Promise<void> {
  try {
    await bus("workspace.delete_collection", { id: c.id });
  } catch (e) {
    if (!(e instanceof BusError) || e.error.code !== "needs_approval") throw e;
    const rule = e.error.rule;
    if (!(await confirm(c.name))) return;
    await bus("workspace.delete_collection", { id: c.id }, { approved: [rule] });
  }
  if (filter().collection === c.id) setFilter({ collection: c.parent ?? null, unfiled: !c.parent });
  await refresh();
}

export async function setPaperTags(paper: string, text: string): Promise<Paper> {
  const p = await bus("workspace.set_tags", { paper, tags: text.split(",") });
  await refresh();
  return p;
}
