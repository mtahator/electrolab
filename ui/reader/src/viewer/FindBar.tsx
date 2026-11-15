// The find bar over the page (design language §17.4): Ctrl+F or the search button. One row:
// the query, "n / m" (growing while pages are searched), previous, next, the full list, close.
// It is a view of the one search (`find.ts`), shared with the Find list in the side panel.

import { IconButton, t } from "@electropaper/kit";
import { type JSX, Show, onMount } from "solid-js";

import { find, setFind, showFindMarks, stepFind } from "./find";

export function FindBar(props: { onList: () => void; onClose: () => void }): JSX.Element {
  let input!: HTMLInputElement;
  onMount(() => {
    input.focus();
    input.select();
    showFindMarks();
  });
  const count = (): string => {
    const n = find.total;
    if (n === null) return "";
    if (n === 0) return find.progress ? "" : t("find.none_short");
    return `${find.active === null ? 0 : find.active + 1} / ${n}`;
  };
  return (
    <div class="find-bar" role="search" aria-label={t("panel.find")}>
      <span class="find-bar-field">
        <input
          ref={input}
          class="find-bar-input"
          placeholder={t("find.placeholder_short")}
          aria-label={t("panel.find")}
          value={find.query}
          onInput={(e) => setFind({ query: e.currentTarget.value })}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              stepFind(e.shiftKey ? -1 : 1);
            } else if (e.key === "Escape") {
              e.preventDefault();
              props.onClose();
            }
          }}
        />
        <span class="find-bar-count" aria-live="polite">
          <Show when={find.progress}>
            {(p) => <span class="spinner" role="img" aria-label={t("find.searching", { done: p().done, total: p().total })} />}
          </Show>
          {count()}
        </span>
      </span>
      <span class="find-bar-sep" />
      <IconButton icon="chevron-up" size="sm" label={t("find.previous")} shortcut="Shift+Enter" disabled={find.hits.length === 0} onClick={() => stepFind(-1)} />
      <IconButton icon="chevron-down" size="sm" label={t("find.next")} shortcut="Enter" disabled={find.hits.length === 0} onClick={() => stepFind(1)} />
      <IconButton icon="list-tree" size="sm" label={t("find.show_list")} shortcut="Ctrl+Shift+F" onClick={() => props.onList()} />
      <IconButton icon="x" size="sm" label={t("common.close")} shortcut="Esc" onClick={() => props.onClose()} />
    </div>
  );
}
