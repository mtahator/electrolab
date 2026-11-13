// Library state and the import flow (the host chains two engines, Draft 2.2 §6.3):
// workspace.add_file → document.metadata → workspace.set_paper_metadata.

import type { Paper, ProjectInfo } from "@electropaper/bindings";
import { createSignal } from "solid-js";

import { bus } from "./host";

const [project, setProject] = createSignal<ProjectInfo | null>(null);
const [papers, setPapers] = createSignal<Paper[]>([]);

export { papers, project };

export async function openDefaultLibrary(): Promise<void> {
  setProject(await bus("workspace.open_default_library", null));
  await refresh();
}

export async function refresh(): Promise<void> {
  setPapers(await bus("workspace.list_papers", { limit: 500, offset: 0 }));
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
