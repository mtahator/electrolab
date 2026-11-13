// Command palette and keyboard-shortcut settings (P1.M2.SH.6), and a confirmation dialog.

import { Button, chord, t } from "@electropaper/kit";
import { For, type JSX, createMemo, createSignal } from "solid-js";

import { type Action, actions, keysOf, setKeys } from "./actions";

function pretty(keys: string | undefined): string {
  return (keys ?? "")
    .split("+")
    .map((k) => (k.length === 1 ? k.toUpperCase() : k.charAt(0).toUpperCase() + k.slice(1)))
    .join("+");
}

export function Palette(props: { onClose: () => void }): JSX.Element {
  const [q, setQ] = createSignal("");
  const [sel, setSel] = createSignal(0);
  const list = createMemo(() => {
    const words = q().toLowerCase().split(/\s+/).filter(Boolean);
    return actions().filter((a) => (a.enabled?.() ?? true) && words.every((w) => a.title().toLowerCase().includes(w)));
  });
  const run = (a: Action | undefined): void => {
    if (!a) return;
    props.onClose();
    queueMicrotask(a.run);
  };
  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="palette" role="dialog" aria-label={t("palette.title")}>
        <input
          class="ep-input"
          placeholder={t("palette.placeholder")}
          ref={(el) => queueMicrotask(() => el.focus())}
          onInput={(e) => {
            setQ(e.currentTarget.value);
            setSel(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
            else if (e.key === "ArrowDown") setSel(Math.min(sel() + 1, list().length - 1));
            else if (e.key === "ArrowUp") setSel(Math.max(sel() - 1, 0));
            else if (e.key === "Enter") run(list()[sel()]);
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        />
        <ul>
          <For each={list()}>
            {(a, i) => (
              <li>
                <button type="button" classList={{ active: i() === sel() }} onMouseEnter={() => setSel(i())} onClick={() => run(a)}>
                  <span>{a.title()}</span>
                  <kbd>{pretty(keysOf(a))}</kbd>
                </button>
              </li>
            )}
          </For>
        </ul>
      </div>
    </div>
  );
}

export function KeymapSettings(props: { onClose: () => void }): JSX.Element {
  const [capturing, setCapturing] = createSignal<string | null>(null);
  const capture = (e: KeyboardEvent): void => {
    const id = capturing();
    if (!id || ["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return void setCapturing(null);
    setKeys(id, chord(e));
    setCapturing(null);
  };
  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()} onKeyDown={capture}>
      <div class="dialog wide" role="dialog" aria-label={t("keymap.title")}>
        <h2>{t("keymap.title")}</h2>
        <table class="keymap">
          <tbody>
            <For each={actions()}>
              {(a) => (
                <tr>
                  <td>{a.title()}</td>
                  <td>
                    <button type="button" class="key-button" onClick={() => setCapturing(a.id)} tabIndex={0}>
                      {capturing() === a.id ? t("keymap.press") : pretty(keysOf(a)) || "—"}
                    </button>
                  </td>
                  <td>
                    <button type="button" class="link-button" onClick={() => setKeys(a.id, null)}>
                      {t("keymap.reset")}
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <div class="dialog-actions">
          <Button onClick={props.onClose}>{t("common.close")}</Button>
        </div>
      </div>
    </div>
  );
}

export function Prompt(props: { message: string; placeholder: string; onAnswer: (value: string | null) => void }): JSX.Element {
  let input!: HTMLInputElement;
  return (
    <div class="overlay" onClick={(e) => e.target === e.currentTarget && props.onAnswer(null)}>
      <div class="dialog" role="dialog" aria-modal="true">
        <label class="prompt-label">
          {props.message}
          <input
            class="ep-input"
            ref={(el) => {
              input = el;
              queueMicrotask(() => el.focus());
            }}
            placeholder={props.placeholder}
            onKeyDown={(e) => {
              if (e.key === "Enter") props.onAnswer(e.currentTarget.value.trim() || null);
              if (e.key === "Escape") props.onAnswer(null);
            }}
          />
        </label>
        <div class="dialog-actions">
          <Button onClick={() => props.onAnswer(null)}>{t("common.cancel")}</Button>
          <Button variant="primary" onClick={() => props.onAnswer(input.value.trim() || null)}>
            {t("common.ok")}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function Confirm(props: { message: string; onAnswer: (ok: boolean) => void }): JSX.Element {
  return (
    <div class="overlay">
      <div class="dialog" role="alertdialog" aria-modal="true">
        <p>{props.message}</p>
        <div class="dialog-actions">
          <Button onClick={() => props.onAnswer(false)}>{t("common.cancel")}</Button>
          <Button variant="primary" onClick={() => props.onAnswer(true)} ref={(el: HTMLButtonElement) => queueMicrotask(() => el.focus())}>
            {t("common.ok")}
          </Button>
        </div>
      </div>
    </div>
  );
}

