// The Notes tab of the right panel (guideline §10.2): the paper's notebook, its history, and the
// margin view switch for sticky notes.

import type { NotesVersion, Paper } from "@electropaper/bindings";
import { IconButton, type MenuItem, openMenu, showToast, t } from "@electropaper/kit";
import { type JSX, Show, createEffect, on } from "solid-js";

import { when } from "../annotations/Editor";
import { bus } from "../host";
import { marginView, toggleMarginView } from "../shell/frame";
import { NotesEditor } from "./NotesEditor";
import { flush, loadNotes, notesOf } from "./state";

export function NotesPanel(props: { paper: Paper }): JSX.Element {
  createEffect(on(() => props.paper.id, (id) => void loadNotes(id)));

  const history = async (at: DOMRect): Promise<void> => {
    await flush(props.paper.id);
    const versions: NotesVersion[] = await bus("workspace.notes_history", { paper: props.paper.id });
    const current = notesOf(props.paper.id)?.version;
    const items: MenuItem[] = versions.length
      ? [
          { heading: t("notes.history") },
          ...versions.slice(0, 20).map((v) => ({
            label: `${when(v.saved_ms)} · ${v.preview || t("notes.empty_version")}`,
            checked: v.version === current,
            run: () =>
              void bus("workspace.restore_notes", { paper: props.paper.id, version: v.version }).then(() => {
                void loadNotes(props.paper.id);
                showToast(t("notes.restored"));
              }),
          })),
        ]
      : [{ heading: t("notes.no_history") }];
    openMenu(t("notes.history"), items, at);
  };

  return (
    <div class="notes-panel-body">
      <div class="notes-actions">
        <span class="muted small" aria-live="polite">
          {notesOf(props.paper.id)?.dirty ? t("notes.saving") : notesOf(props.paper.id)?.version ? t("notes.saved") : ""}
        </span>
        <span class="ep-spacer" />
        <IconButton icon="messages-square" size="sm" label={t("notes.margin_view")} shortcut="Ctrl+Shift+M" pressed={marginView(props.paper.id)} onClick={() => toggleMarginView(props.paper.id)} />
        <IconButton icon="history" size="sm" label={t("notes.history")} onClick={(e) => void history(e.currentTarget.getBoundingClientRect())} />
      </div>
      {/* One editor per paper: switching papers starts a fresh editor (and its own undo). */}
      <Show when={notesOf(props.paper.id)?.loaded ? props.paper.id : null} keyed>
        {(id) => <NotesEditor paper={id} />}
      </Show>
    </div>
  );
}
