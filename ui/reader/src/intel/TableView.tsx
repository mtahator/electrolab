// A table as cells (P1.M4.TAB.2): sortable by any column, numbers right-aligned; copy or save as
// CSV. Asking it (P1.M5.UND.6; design language §17.13): the answer is worked out by code from the
// cells, which are marked; a click on the answer goes to its cell.

import { type BlobRef, BusError, type Cell, type DocObject, type Table, type TableAnswer } from "@electropaper/bindings";
import { Button, Icon, IconButton, Skeleton, showToast, t } from "@electropaper/kit";
import { save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { For, type JSX, Show, createMemo, createResource, createSignal, onCleanup, onMount } from "solid-js";

import { canExplain } from "../assistant/explain";
import { bus, cancelCall } from "../host";

/** A row with its place in the table, kept through sorting. */
interface Row {
  row: Cell[];
  r: number;
}

export function TableView(props: { file: BlobRef; object: DocObject; ask?: boolean; onClose: () => void }): JSX.Element {
  let scroll: HTMLDivElement | undefined;
  let box!: HTMLDivElement;
  let input: HTMLInputElement | undefined;
  const object = (): { page: number; id: string } => ({ page: props.object.rect.page, id: props.object.id });
  const [table] = createResource(() => bus("document.table", { file: props.file, object: object() }).catch((e: unknown) => ({ error: String((e as Error).message ?? e) })));
  const [sort, setSort] = createSignal<{ col: number; dir: 1 | -1 } | null>(null);
  const t0 = (): Table | null => {
    const v = table();
    return v && "rows" in v ? v : null;
  };
  const heads = (tb: Table): number => Math.max(1, tb.header_rows);
  const body = createMemo<Row[]>(() => {
    const tb = t0();
    if (!tb) return [];
    const rows = tb.rows.map((row, r) => ({ row, r })).slice(heads(tb));
    const s = sort();
    if (!s) return rows;
    return [...rows].sort((a, b) => {
      const x = a.row[s.col];
      const y = b.row[s.col];
      const c = x?.value != null && y?.value != null ? x.value - y.value : (x?.text ?? "").localeCompare(y?.text ?? "");
      return c * s.dir;
    });
  });

  // ---- asking ----
  const [question, setQuestion] = createSignal("");
  const [asking, setAsking] = createSignal<string | null>(null);
  const [answer, setAnswer] = createSignal<TableAnswer | null>(null);
  const [failed, setFailed] = createSignal("");
  const roles = createMemo(() => {
    const m = new Map<string, "answer" | "used">();
    for (const c of answer()?.cells ?? []) m.set(`${c.row}:${c.col}`, c.role);
    return m;
  });
  const stop = (): void => {
    const id = asking();
    if (id) void cancelCall(id);
    setAsking(null);
  };
  const ask = async (): Promise<void> => {
    const q = question().trim();
    if (!q || asking()) return;
    const id = crypto.randomUUID();
    setAsking(id);
    setAnswer(null);
    setFailed("");
    try {
      const a = await bus("understanding.table_answer", { file: props.file, object: object(), question: q }, { id });
      if (asking() !== id) return;
      setAnswer(a);
      show(a);
    } catch (e) {
      const err = e instanceof BusError ? e.error : null;
      const kind = err?.code === "failed" ? err.kind : "";
      if (asking() === id) setFailed(kind === "no_model" || kind === "no_runtime" ? t("explain.no_model") : String((e as Error).message ?? e));
    } finally {
      setAsking((x) => (x === id ? null : x));
    }
  };
  /** Bring the answer's cell into view. */
  const show = (a: TableAnswer): void => {
    const c = a.cells.find((x) => x.role === "answer") ?? a.cells[0];
    if (c) scroll?.querySelector(`[data-cell="${c.row}:${c.col}"]`)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  };
  const says = (a: TableAnswer): string => (a.status === "computed" ? a.text : t(`table.reason.${a.reason ?? "not_in_table"}`));

  onMount(() => {
    (props.ask && input ? input : box).focus();
    onCleanup(stop);
  });

  const saveCsv = async (): Promise<void> => {
    const tb = t0();
    if (!tb) return;
    const path = await save({ title: t("intel.save_csv"), defaultPath: `${(props.object.label ?? "table").replace(/\s+/g, "-")}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    await invoke("save_text_file", { path, contents: tb.csv });
    showToast(t("intel.saved_csv"));
  };
  const cell = (c: Cell, r: number, ci: number): { "data-cell": string; "data-role"?: string; "aria-label"?: string } => {
    const role = roles().get(`${r}:${ci}`);
    return { "data-cell": `${r}:${ci}`, ...(role ? { "data-role": role, "aria-label": `${c.text} (${t(`table.role.${role}`)})` } : {}) };
  };
  return (
    <div class="ep-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="table-view" role="dialog" aria-label={props.object.label ?? t("intel.table")} ref={box} tabIndex={-1} onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), asking() ? stop() : props.onClose())}>
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
        <Show when={canExplain()}>
          <form
            class="table-ask"
            onSubmit={(e) => {
              e.preventDefault();
              void ask();
            }}
          >
            <Icon name="sparkles" size={16} />
            <input class="ep-input" type="text" ref={input} value={question()} aria-label={t("table.ask")} placeholder={t("table.ask_placeholder")} onInput={(e) => setQuestion(e.currentTarget.value)} />
            <Show when={asking()} fallback={<Button type="submit" disabled={!question().trim() || !t0()}>{t("table.ask")}</Button>}>
              <Button icon="x" onClick={stop}>
                {t("explain.stop")}
              </Button>
            </Show>
          </form>
          <div class="table-said" role="status">
            <Show when={asking()}>
              <span class="spinner" aria-hidden="true" />
              <span class="muted">{t("table.looking")}</span>
            </Show>
            <Show when={answer()}>
              {(a) => (
                <Show when={a().cells.length > 0} fallback={<span>{says(a())}</span>}>
                  <button type="button" class="table-answer" data-tip={t("table.show_cell")} onClick={() => show(a())}>
                    {says(a())}
                  </button>
                </Show>
              )}
            </Show>
            <Show when={failed()}>
              <span>{failed()}</span>
            </Show>
          </div>
        </Show>
        <Show when={table()} fallback={<Skeleton lines={6} />}>
          <Show when={t0()} fallback={<p class="muted">{(table() as { error: string }).error}</p>}>
            {(tb) => (
              <div class="table-scroll" ref={scroll}>
                <table class="data-table">
                  <thead>
                    <For each={tb().rows.slice(0, heads(tb()))}>
                      {(row, ri) => (
                        <tr>
                          <For each={row}>
                            {(c, ci) => (
                              <th {...cell(c, ri(), ci())}>
                                <Show when={ri() === heads(tb()) - 1} fallback={c.text}>
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
                    <For each={body()}>
                      {(x) => (
                        <tr>
                          <For each={x.row}>
                            {(c, ci) => (
                              <td classList={{ num: c.value != null }} {...cell(c, x.r, ci())}>
                                {c.text}
                              </td>
                            )}
                          </For>
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
