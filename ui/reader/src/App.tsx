import type { Paper } from "@electropaper/bindings";
import { BusError } from "@electropaper/bindings";
import { Button, THEMES, type Theme, bind, setTheme, t, theme } from "@electropaper/kit";
import { type JSX, For, Show, createSignal, onCleanup, onMount } from "solid-js";

import { onBusEvent, pickPdfs } from "./host";
import { importPdf, openDefaultLibrary, papers, project, refresh } from "./library";
import { PageView } from "./PageView";

export function App(): JSX.Element {
  const [selected, setSelected] = createSignal<Paper | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const report = (e: unknown) => setError(e instanceof BusError ? e.message : String(e));

  async function addPdfs(): Promise<void> {
    const files = await pickPdfs(t("library.add"));
    if (files.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      let last: Paper | null = null;
      for (const f of files) last = await importPdf(f);
      await refresh();
      if (last) setSelected(last);
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  }

  onMount(async () => {
    try {
      await openDefaultLibrary();
      const first = papers()[0];
      if (first) setSelected(first);
    } catch (e) {
      report(e);
    }
    const off = await onBusEvent("workspace.paper", () => void refresh());
    const unbind = bind("Ctrl+O", () => void addPdfs());
    onCleanup(() => {
      off();
      unbind();
    });
  });

  return (
    <div class="app">
      <header class="topbar">
        <strong>{t("app.title")}</strong>
        <span class="muted">{project()?.name}</span>
        <span class="spacer" />
        <Button variant="primary" disabled={busy()} onClick={() => void addPdfs()} title="Ctrl+O">
          {busy() ? t("library.adding") : t("library.add")}
        </Button>
        <label class="field">
          {t("settings.theme")}
          <select class="ep-select" value={theme()} onChange={(e) => setTheme(e.currentTarget.value as Theme)}>
            <For each={THEMES}>{(th) => <option value={th}>{t(`theme.${th}`)}</option>}</For>
          </select>
        </label>
      </header>
      <Show when={error()}>
        <div class="banner" role="alert">
          <strong>{t("error.title")}:</strong> {error()}
        </div>
      </Show>
      <aside class="library">
        <h2>{t("library.title")}</h2>
        <Show when={papers().length > 0} fallback={<p class="muted">{t("library.empty")}</p>}>
          <ul>
            <For each={papers()}>
              {(p) => (
                <li>
                  <button type="button" class="paper" classList={{ active: selected()?.id === p.id }} onClick={() => setSelected(p)}>
                    <span class="paper-title">{p.title ?? p.original_name ?? t("library.untitled")}</span>
                    <span class="muted">
                      {(p.authors ?? []).slice(0, 3).join(", ")}
                      {p.pages ? ` · ${t("library.pages", { n: p.pages })}` : ""}
                    </span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </aside>
      <main class="viewer">
        <Show when={selected()} fallback={<p class="muted center">{t("viewer.empty")}</p>}>
          {(p) => <PageView hash={p().file.hash} />}
        </Show>
      </main>
    </div>
  );
}
