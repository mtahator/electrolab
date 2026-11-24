// Settings (guideline §2): appearance, and the page models — which are present, getting the
// optional layout model, removing it again. Models shipped with the app are listed but fixed.

import type { ModelInfo } from "@electropaper/bindings";
import { Button, Dialog, Icon, type MessageKey, PAPERS, type PaperChoice, TEXT_SIZES, THEMES, type TextSize, type Theme, density, paperChoice, setDensity, setPaper, setTextSize, setTheme, t, textSize, theme } from "@electropaper/kit";
import { For, type JSX, Show, createSignal } from "solid-js";

import { download, fetchModel, models, removeModel } from "../intel/state";
import { LanguageModels } from "./LanguageModels";

/** The models a person thinks of: one row per model, its files together (size summed,
 *  present only when every file is). */
const shown = (ms: ModelInfo[]): ModelInfo[] =>
  ms
    .filter((m) => m.id === m.group)
    .map((m) => {
      const files = ms.filter((f) => f.group === m.group);
      return { ...m, size: files.reduce((a, f) => a + f.size, 0), present: files.every((f) => f.present) };
    });

export function Settings(props: { onClose: () => void; onModelReady: () => void }): JSX.Element {
  const [busy, setBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const mb = (n: number): number => Math.max(1, Math.round(n / 1e6));

  const get = async (id: string): Promise<void> => {
    setError(null);
    if (await fetchModel(id)) props.onModelReady();
    else setError(download()?.error ?? t("intel.model_failed"));
  };
  const remove = async (id: string): Promise<void> => {
    setError(null);
    setBusy(id);
    try {
      await removeModel(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog size="lg" title={t("settings.title")} onCancel={props.onClose} actions={<Button onClick={props.onClose}>{t("common.close")}</Button>}>
      <div class="settings">
        <section>
          <h3>{t("settings.appearance")}</h3>
          <Choice label={t("settings.app_theme")} value={theme()} options={THEMES.map((th) => [th, t(`theme.${th}`)] as const)} onChange={(v) => setTheme(v as Theme)} />
          <Choice
            label={t("settings.paper")}
            value={paperChoice()}
            options={[["auto", t("paper.auto")] as const, ...PAPERS.map((p) => [p, t(`paper.${p}`)] as const)]}
            onChange={(v) => setPaper(v as PaperChoice)}
          />
          <Choice label={t("settings.text_size")} value={String(textSize())} options={TEXT_SIZES.map((n) => [String(n), `${n} %`] as const)} onChange={(v) => setTextSize(Number(v) as TextSize)} />
          <label class="settings-row">
            <span>{t("settings.compact")}</span>
            <input type="checkbox" checked={density() === "compact"} onChange={(e) => setDensity(e.currentTarget.checked ? "compact" : "comfortable")} />
          </label>
        </section>
        <section>
          <h3>{t("settings.models")}</h3>
          <p class="muted">{t("settings.models_note")}</p>
          <ul class="model-list">
            <For each={shown(models())}>
              {(m) => (
                <li class="model-row">
                  <Icon name={m.present ? "circle-check" : "circle"} size={16} />
                  <div class="model-text">
                    <strong>{t(`model.${m.id}` as MessageKey)}</strong>
                    <span class="muted">
                      {m.purpose} · {mb(m.size)} MB · {m.licence}
                    </span>
                  </div>
                  <Show
                    when={!m.bundled}
                    fallback={<span class="muted model-state">{t("settings.model_included")}</span>}
                  >
                    <Show
                      when={m.present}
                      fallback={
                        <Show when={download()?.id === m.id && !download()?.error} fallback={<Button icon="file-down" onClick={() => void get(m.id)}>{t("intel.download", { mb: mb(m.size) })}</Button>}>
                          <span class="model-state">{t("intel.downloading", { percent: download()?.total ? Math.round((100 * (download()?.received ?? 0)) / (download()?.total ?? 1)) : 0 })}</span>
                        </Show>
                      }
                    >
                      <Button icon="trash" disabled={busy() === m.id} onClick={() => void remove(m.id)}>
                        {t("settings.model_remove")}
                      </Button>
                    </Show>
                  </Show>
                </li>
              )}
            </For>
          </ul>
          <Show when={error()}>
            <p class="error">{error()}</p>
          </Show>
        </section>
        <LanguageModels />
      </div>
    </Dialog>
  );
}

/** A labelled choice, one row of Settings. */
export function Choice(props: { label: string; value: string; options: readonly (readonly [string, string])[]; onChange: (v: string) => void }): JSX.Element {
  return (
    <label class="settings-row">
      <span>{props.label}</span>
      <select class="ep-input" value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
        <For each={props.options}>{([v, label]) => <option value={v}>{label}</option>}</For>
      </select>
    </label>
  );
}
