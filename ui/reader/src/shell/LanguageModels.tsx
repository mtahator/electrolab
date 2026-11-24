// The language models part of Settings: the folders that are searched, the order in which
// models are tried, and every model found (engines/inference/CONTRACT.md, automatic model search).

import type { LlmFolder, LlmModel, LlmModels, LlmStatus } from "@electropaper/bindings";
import { Button, Icon, IconButton, type MessageKey, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onMount } from "solid-js";

import { addModelFolder, chooseModels, languageModels, removeModelFolder, searchModels } from "../assistant/explain";
import { setUnderstandOn, understandOn } from "../assistant/understand";
import { bus, pickFolder } from "../host";
import { Choice } from "./Settings";

const size = (n: number): string => (n >= 1e9 ? `${(n / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(n / 1e6))} MB`);

export function LanguageModels(): JSX.Element {
  const [searching, setSearching] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  /** Run one change; the list shows its result, and the person sees that something happens. */
  const run = async (f: () => Promise<unknown>): Promise<void> => {
    setError(null);
    setSearching(true);
    try {
      await f();
    } catch (e) {
      setError(String((e as Error).message ?? e));
    } finally {
      setSearching(false);
    }
  };
  onMount(() => void run(searchModels));

  const all = (): LlmModels | null => languageModels();
  const chat = (): LlmModel[] => (all()?.models ?? []).filter((m) => m.kind === "chat");
  const others = (): LlmModel[] => (all()?.models ?? []).filter((m) => m.kind !== "chat");
  const name = (id: string): string => chat().find((m) => m.id === id)?.name ?? id;
  const order = (): string[] => (all()?.order ?? []).filter((id) => chat().some((m) => m.id === id));
  const left = (): LlmModel[] => chat().filter((m) => !order().includes(m.id));
  const choose = (list: string[]): void => void run(() => chooseModels({ order: list }));
  const move = (i: number, by: number): void => {
    const list = [...order()];
    const [x] = list.splice(i, 1);
    if (x) list.splice(i + by, 0, x);
    choose(list);
  };
  const place = (i: number): string => (i === 0 ? t("llm.first") : i === 1 ? t("llm.second") : t("llm.next", { n: i + 1 }));
  const note = (id: string): string => {
    const m = chat().find((x) => x.id === id);
    if (!m) return "";
    const parts = [t("llm.needs", { size: size(m.needs) })];
    if (m.state === "loaded") parts.push(t("settings.llm_loaded"));
    else if (m.problem) parts.push(m.problem);
    return parts.join(" · ");
  };
  const [status, setStatus] = createSignal<LlmStatus | null>(null);
  onMount(() => void bus("inference.status", null).then(setStatus).catch(() => {}));

  const folderNote = (f: LlmFolder): string => {
    if (f.missing) return t("llm.folder_missing");
    const parts = [f.models === 1 ? t("llm.found_one") : t("llm.found", { n: f.models })];
    if (f.other_formats > 0) parts.push(t("llm.other_formats", { n: f.other_formats }));
    if (f.partial) parts.push(t("llm.partial"));
    return parts.join(" · ");
  };

  return (
    <section class="llm">
      <h3>{t("settings.llm")}</h3>
      <p class="muted">{t("settings.llm_note")}</p>

      <ul class="model-list">
        <For each={all()?.folders ?? []}>
          {(f) => (
            <li class="model-row">
              <Icon name="folder" size={16} />
              <div class="model-text">
                <span class="llm-path">{f.path}</span>
                <span class="muted">{folderNote(f)}</span>
              </div>
              <Show when={f.own}>
                <Button icon="x" disabled={searching()} onClick={() => void run(() => removeModelFolder(f.path))}>
                  {t("llm.forget_folder")}
                </Button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <div class="row llm-actions">
        <Button
          icon="folder"
          disabled={searching()}
          onClick={() =>
            void run(async () => {
              const dir = await pickFolder(t("settings.llm_folder"));
              if (dir) await addModelFolder(dir);
            })
          }
        >
          {t("settings.llm_folder")}
        </Button>
        <Button variant="quiet" disabled={searching()} onClick={() => void run(searchModels)}>
          {t("llm.search_again")}
        </Button>
        <Show when={searching()}>
          <span class="muted llm-searching" role="status">
            <span class="spinner" aria-hidden="true" /> {t("llm.searching")}
          </span>
        </Show>
      </div>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
      <Show when={all()?.runtime.backend === "none"}>
        <p class="error">{t("settings.llm_no_runtime")}</p>
      </Show>

      <Show when={chat().length > 0} fallback={<Show when={!searching()}><p class="muted">{t("llm.none")}</p></Show>}>
        <h4>{t("llm.order")}</h4>
        <p class="muted">{t("llm.order_note")}</p>
        <ol class="model-list llm-order">
          <For each={order()}>
            {(id, i) => (
              <li class="model-row">
                <div class="model-text">
                  <span class="muted">{place(i())}</span>
                  <strong>{name(id)}</strong>
                  <span class="muted">{note(id)}</span>
                </div>
                <div class="row">
                  <IconButton icon="chevron-up" size="sm" label={t("llm.up", { model: name(id) })} disabled={i() === 0 || searching()} onClick={() => move(i(), -1)} />
                  <IconButton icon="chevron-down" size="sm" label={t("llm.down", { model: name(id) })} disabled={i() === order().length - 1 || searching()} onClick={() => move(i(), 1)} />
                  <IconButton icon="x" size="sm" label={t("llm.leave_out", { model: name(id) })} disabled={order().length < 2 || searching()} onClick={() => choose(order().filter((x) => x !== id))} />
                </div>
              </li>
            )}
          </For>
        </ol>
        <Show when={left().length > 0}>
          <Choice label={t("llm.add")} value="" options={[["", t("llm.add_pick")] as const, ...left().map((m) => [m.id, m.name] as const)]} onChange={(id) => id && choose([...order(), id])} />
        </Show>
        <Show when={all()?.own_order}>
          <Button variant="quiet" disabled={searching()} onClick={() => choose([])}>
            {t("llm.automatic")}
          </Button>
        </Show>
        <Show when={status()?.fallback && status()?.model}>
          <p class="muted">{t(`llm.fallback.${status()?.fallback ?? "start_failed"}`, { wanted: name(status()?.wanted ?? ""), used: name(status()?.model ?? "") })}</p>
        </Show>
        <Show when={all()?.memory}>{(m) => <p class="muted">{t("llm.memory", { device: m().device, free: size(m().free), total: size(m().total) })}</p>}</Show>
        <label class="settings-row">
          <span>{t("llm.keep_ready")}</span>
          <input type="checkbox" checked={all()?.keep_ready ?? true} onChange={(e) => void run(() => chooseModels({ keep_ready: e.currentTarget.checked }))} />
        </label>
        <label class="settings-row">
          <span>{t("brain.setting")}</span>
          <input type="checkbox" checked={understandOn()} onChange={(e) => setUnderstandOn(e.currentTarget.checked)} />
        </label>
        <h4>{t("llm.models")}</h4>
        <ul class="model-list">
          <For each={[...chat(), ...others()]}>
            {(m) => (
              <li class="model-row" classList={{ unusable: m.kind !== "chat" }}>
                <Icon name={m.kind === "chat" ? "circle-check" : "circle"} size={16} />
                <div class="model-text">
                  <strong>{m.name}</strong>
                  <span class="muted">{[size(m.size), m.architecture, m.licence].filter(Boolean).join(" · ")}</span>
                  <span class="muted llm-path">{m.file}</span>
                  <Show when={m.problem}>
                    <span class="error">{t("llm.problem", { why: m.problem ?? "" })}</span>
                  </Show>
                </div>
                <span class="muted model-state">
                  {m.kind !== "chat" ? t(`llm.kind.${m.kind}` as MessageKey) : m.state === "loaded" ? t("settings.llm_loaded") : m.recommended ? t("llm.suggested") : t("settings.llm_ready")}
                </span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
