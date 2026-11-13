// Library state and the import flow (the host chains two engines, Draft 2.2 §6.3):
// workspace.add_file → document.metadata → workspace.set_paper_metadata.

import type { Collection, Paper, PaperSort, ProjectInfo, TagCount } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus } from "./host";

export interface Filter {
  query: string;
  collection: string | null;
  tag: string | null;
  sort: PaperSort;
}

const [project, setProject] = createSignal<ProjectInfo | null>(null);
const [papers, setPapers] = createSignal<Paper[]>([]);
const [collections, setCollections] = createSignal<Collection[]>([]);
const [tags, setTags] = createSignal<TagCount[]>([]);
const [filter, setFilterSignal] = createSignal<Filter>({ query: "", collection: null, tag: null, sort: "added" });

export { collections, filter, papers, project, tags };

export async function openDefaultLibrary(): Promise<void> {
  setProject(await bus("workspace.open_default_library", null));
  await refresh();
}

/** Reload papers (with the current filter), collections and tags. */
export async function refresh(): Promise<void> {
  const f = filter();
  const [p, c, t] = await Promise.all([
    bus("workspace.list_papers", { limit: 1000, offset: 0, query: f.query || null, collection: f.collection, tag: f.tag, sort: f.sort }),
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
export async function importPdf(path: string): Promise<Paper> {
  const added = await bus("workspace.add_file", { path });
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

export async function createCollection(name: string): Promise<void> {
  await bus("workspace.create_collection", { name });
  await refresh();
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
  if (filter().collection === c.id) setFilter({ collection: null });
  await refresh();
}

export async function addToCollection(collection: string, paper: string): Promise<void> {
  await bus("workspace.add_to_collection", { collection, papers: [paper] });
  await refresh();
}

export async function setPaperTags(paper: string, text: string): Promise<Paper> {
  const p = await bus("workspace.set_tags", { paper, tags: text.split(",") });
  await refresh();
  return p;
}
