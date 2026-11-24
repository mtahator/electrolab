// Suggested folders for the selected papers (design language §17.10): the groups are shown first;
// names can be changed, papers put elsewhere, groups left out. Only the person's approval moves them.

import { BusError, type FolderSuggestion, type Paper } from "@electropaper/bindings";
import { Button, Dialog, Icon, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onCleanup, onMount } from "solid-js";
import { createStore, produce } from "solid-js/store";

import { languageModels } from "../assistant/explain";
import { bus, cancelCall, onBusEvent } from "../host";
import { setSettingsOpen } from "../intel/state";
import { type FolderGroup, collections, pathOf, titleOf } from "../library";

interface Group {
  name: string;
  existing: string | null;
  take: boolean;
  papers: string[];
}

export function SortSheet(props: {
  papers: Paper[];
  /** Folder in which new folders are made. */
  parent: string | null;
  onApply: (groups: FolderGroup[]) => void;
  onClose: () => void;
}): JSX.Element {
  const id = crypto.randomUUID();
  const [state, setState] = createSignal<"running" | "ready" | "failed">("running");
  const [progress, setProgress] = createSignal<{ step: string; done: number; total: number } | null>(null);
  const [error, setError] = createSignal<{ kind: string; message: string } | null>(null);
  const [groups, setGroups] = createStore<Group[]>([]);
  const [unsure, setUnsure] = createSignal<string[]>([]);
  const [model, setModel] = createSignal("");
  const paper = (p: string): Paper | undefined => props.papers.find((x) => x.id === p);
  let open = true;

  onMount(async () => {
    const stop = await onBusEvent("understanding.sorting", (e) => {
      const d = e.data as { id: string; step: string; done: number; total: number };
      if (d.id === id) setProgress(d);
    });
    onCleanup(stop);
    try {
      const r: FolderSuggestion = await bus(
        "understanding.suggest_folders",
        {
          papers: props.papers.map((p) => ({ id: p.id, title: titleOf(p), file: p.file })),
          folders: collections().filter((c) => (c.parent ?? null) === props.parent).map((c) => ({ id: c.id, name: c.name })),
          stream: { id },
        },
        { id },
      );
      if (!open) return;
      setGroups(r.folders.map((f) => ({ name: f.name, existing: f.existing ?? null, take: true, papers: f.papers })));
      setUnsure(r.unsure);
      setModel(r.model);
      setState("ready");
    } catch (e) {
      if (!open) return;
      const err = e instanceof BusError ? e.error : null;
      setError({ kind: err?.code === "failed" ? err.kind : (err?.code ?? ""), message: String((e as Error).message ?? e) });
      setState("failed");
    }
  });
  const close = (): void => {
    open = false;
    if (state() === "running") void cancelCall(id);
    props.onClose();
  };
  onCleanup(() => {
    open = false;
  });

  /** Put a paper into another group (-1: stay where it is). */
  const put = (p: string, to: number): void => {
    setGroups(
      produce((all) => {
        for (const g of all) g.papers = g.papers.filter((x) => x !== p);
        all[to]?.papers.push(p);
      }),
    );
    setUnsure((u) => (to < 0 ? [...u.filter((x) => x !== p), p] : u.filter((x) => x !== p)));
  };
  const chosen = (): Group[] => groups.filter((g) => g.take && g.papers.length > 0 && g.name.trim());
  const moving = (): number => chosen().reduce((n, g) => n + g.papers.length, 0);
  const step = (): string => {
    const p = progress();
    if (!p || p.step === "thinking") return t("sort.thinking");
    return t(p.step === "reading" ? "sort.reading" : "sort.sorting", { done: Math.min(p.done + 1, p.total), total: p.total });
  };
  const needsModel = (): boolean => ["no_model", "no_runtime"].includes(error()?.kind ?? "");
  const modelName = (): string => languageModels()?.models.find((m) => m.id === model())?.name ?? model();

  const Row = (r: { id: string; at: number }): JSX.Element => (
    <li class="sort-paper">
      <span class="sort-title">{titleOf(paper(r.id))}</span>
      <select class="ep-select" aria-label={t("sort.put", { title: titleOf(paper(r.id)) })} value={String(r.at)} onChange={(e) => put(r.id, Number(e.currentTarget.value))}>
        <For each={groups}>{(g, i) => <option value={String(i())}>{g.name}</option>}</For>
        <option value="-1">{t("sort.stay")}</option>
      </select>
    </li>
  );

  return (
    <Dialog
      size="lg"
      title={t("sort.title", { n: props.papers.length })}
      onCancel={close}
      actions={
        <>
          <Button onClick={close}>{state() === "running" ? t("explain.stop") : t("common.cancel")}</Button>
          <Show when={state() === "ready" && groups.length > 0}>
            <Button
              variant="primary"
              icon="folder"
              disabled={moving() === 0}
              onClick={() => {
                open = false;
                props.onApply(chosen().map((g) => (g.existing ? { papers: g.papers, collection: g.existing } : { papers: g.papers, new: { name: g.name.trim(), parent: props.parent } })));
              }}
            >
              {t("sort.apply", { n: moving() })}
            </Button>
          </Show>
        </>
      }
    >
      <div class="sort-sheet">
        <Show when={state() === "running"}>
          <p class="muted sort-status" role="status">
            <span class="spinner" aria-hidden="true" /> {step()}
          </p>
        </Show>
        <Show when={state() === "failed"}>
          <p>{needsModel() ? t("explain.no_model") : error()?.message}</p>
          <Show when={needsModel()}>
            <Button icon="settings" onClick={() => (close(), setSettingsOpen(true))}>
              {t("explain.open_settings")}
            </Button>
          </Show>
        </Show>
        <Show when={state() === "ready"}>
          <p class="muted">{groups.length > 0 ? t("sort.note") : t("sort.none")}</p>
          <For each={groups}>
            {(g, i) => (
              <section class="sort-group" classList={{ left: !g.take }}>
                <div class="sort-head">
                  <input type="checkbox" checked={g.take} aria-label={t("sort.take", { name: g.name })} onChange={(e) => setGroups(i(), "take", e.currentTarget.checked)} />
                  <Icon name="folder" size={16} />
                  <Show when={!g.existing} fallback={<strong class="sort-name">{g.name}</strong>}>
                    <input class="ep-input sort-name" value={g.name} aria-label={t("library.folder_name")} onInput={(e) => setGroups(i(), "name", e.currentTarget.value)} />
                  </Show>
                  <span class="muted">{g.existing ? t("sort.existing", { n: g.papers.length }) : t("sort.new", { n: g.papers.length })}</span>
                </div>
                <ul class="sort-list">
                  <For each={g.papers}>{(p) => <Row id={p} at={i()} />}</For>
                </ul>
              </section>
            )}
          </For>
          <Show when={unsure().length > 0}>
            <section class="sort-group left">
              <div class="sort-head">
                <strong class="sort-name">{t("sort.unsure")}</strong>
              </div>
              <ul class="sort-list">
                <For each={unsure()}>{(p) => <Row id={p} at={-1} />}</For>
              </ul>
            </section>
          </Show>
          <p class="muted explain-status">
            {props.parent ? `${t("sort.inside", { path: pathOf(props.parent) })} · ` : ""}
            {t("explain.generated", { model: modelName() })}
          </p>
        </Show>
      </div>
    </Dialog>
  );
}
