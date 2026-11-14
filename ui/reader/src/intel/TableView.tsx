// A table as cells (P1.M4.TAB.2): sortable by any column, numbers right-aligned; copy or save as
// CSV.

import type { BlobRef, DocObject, Table } from "@electropaper/bindings";
import { Button, Icon, IconButton, Skeleton, showToast, t } from "@electropaper/kit";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { For, type JSX, Show, createMemo, createResource, createSignal } from "solid-js";

import { bus } from "../host";

export function TableView(props: { file: BlobRef; object: DocObject; onClose: () => void }): JSX.Element {
  const [table] = createResource(() => bus("document.table", { file: props.file, object: { page: props.object.rect.page, id: props.object.id } }).catch((e: unknown) => ({ error: String((e as Error).message ?? e) })));
  const [sort, setSort] = createSignal<{ col: number; dir: 1 | -1 } | null>(null);
  const t0 = (): Table | null => {
    const v = table();
    return v && "rows" in v ? v : null;
  };
  const body = createMemo(() => {
    const tb = t0();
    if (!tb) return [];
    const rows = tb.rows.slice(tb.header_rows);
    const s = sort();
    if (!s) return rows;
    return [...rows].sort((a, b) => {
      const x = a[s.col];
      const y = b[s.col];
      const c = x?.value != null && y?.value != null ? x.value - y.value : (x?.text ?? "").localeCompare(y?.text ?? "");
      return c * s.dir;
    });
  });
  const saveCsv = async (): Promise<void> => {
    const tb = t0();
    if (!tb) return;
    const path = await save({ title: t("intel.save_csv"), defaultPath: `${(props.object.label ?? "table").replace(/\s+/g, "-")}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    await invoke("save_text_file", { path, contents: tb.csv });
    showToast(t("intel.saved_csv"));
  };
  return (
    <div class="ep-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="table-view" role="dialog" aria-label={props.object.label ?? t("intel.table")} onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), props.onClose())}>
        <header class="row">
          <Icon name="table" size={16} />
          <strong>{props.object.label ?? t("intel.table")}</strong>
          <span class="ep-spacer" />
          <Button icon="copy" disabled={!t0()} onClick={() => void navigator.clipboard.writeText(t0()?.csv ?? "").then(() => showToast(t("intel.copied_csv")))}>
            {t("intel.copy_csv")}
          </Button>
          <Button icon="file-down" disabled={!t0()} onClick={() => void saveCsv().catch((e: unknown) => showToast(String(e)))}>
            {t("intel.save_csv")}
          </Button>
          <IconButton icon="x" label={t("common.close")} shortcut="Esc" onClick={props.onClose} />
        </header>
        <Show when={table()} fallback={<Skeleton lines={6} />}>
          <Show when={t0()} fallback={<p class="muted">{(table() as { error: string }).error}</p>}>
            {(tb) => (
              <div class="table-scroll">
                <table class="data-table">
                  <thead>
                    <For each={tb().rows.slice(0, Math.max(1, tb().header_rows))}>
                      {(row, ri) => (
                        <tr>
                          <For each={row}>
                            {(c, ci) => (
                              <th>
                                <Show when={ri() === Math.max(1, tb().header_rows) - 1} fallback={c.text}>
                                  <button
                                    type="button"
                                    class="sort"
                                    aria-label={`${t("intel.sort_by")} ${c.text}`}
                                    onClick={() => setSort((s) => (s && s.col === ci() ? (s.dir === 1 ? { col: ci(), dir: -1 } : null) : { col: ci(), dir: 1 }))}
                                  >
                                    {c.text}
                                    <Show when={sort()?.col === ci()}>
                                      <Icon name={sort()?.dir === 1 ? "chevron-down" : "chevron-right"} size={12} />
                                    </Show>
                                  </button>
                                </Show>
                              </th>
                            )}
                          </For>
                        </tr>
                      )}
                    </For>
                  </thead>
                  <tbody>
                    <For each={tb().header_rows === 0 ? tb().rows.slice(1) : body()}>
                      {(row) => (
                        <tr>
                          <For each={row}>{(c) => <td classList={{ num: c.value != null }}>{c.text}</td>}</For>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            )}
          </Show>
        </Show>
      </div>
    </div>
  );
}
