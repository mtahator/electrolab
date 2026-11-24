// The language models part of Settings: the folders that are searched, the model for each kind of
// work, and every model found (engines/inference/CONTRACT.md, automatic model search).

import type { LlmFolder, LlmModel, LlmModels } from "@electropaper/bindings";
import { Button, Icon, type MessageKey, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onMount } from "solid-js";

import { addModelFolder, assignModel, languageModels, removeModelFolder, searchModels } from "../assistant/explain";
import { pickFolder } from "../host";
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
  const suggested = (): LlmModel | undefined => chat().find((m) => m.recommended);
  const chosen = (task: string): string => {
    const id = all()?.tasks.find((x) => x.id === task)?.model ?? "";
    return chat().some((m) => m.id === id) ? id : "";
  };
  const options = (task: string): (readonly [string, string])[] => [
    ["", task === "default" ? t("llm.suggested_option", { model: suggested()?.name ?? "" }) : t("llm.same_option")] as const,
    ...chat().map((m) => [m.id, m.name] as const),
  ];
  const explain = (): string => chosen("explain") || chosen("default");
  const check = (): string => chosen("check") || chosen("default");

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
        <h4>{t("llm.tasks")}</h4>
        <For each={["default", "explain", "check", "digest", "find", "sort"]}>
          {(task) => <Choice label={t(`llm.task.${task}` as MessageKey)} value={chosen(task)} options={options(task)} onChange={(id) => void run(() => assignModel(task, id || null))} />}
        </For>
        <Show when={explain() !== check()}>
          <p class="muted">{t("llm.two_models")}</p>
        </Show>
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
