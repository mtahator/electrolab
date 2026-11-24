// Command palette and keyboard-shortcut settings (P1.M2.SH.6), and a one-line prompt; built on
// the kit's dialog (guideline §8).

import { Button, Dialog, Icon, Kbd, chord, t } from "@electropaper/kit";
import { For, type JSX, Show, createMemo, createSignal } from "solid-js";

import { type Action, actions, keysOf, setKeys } from "./actions";

export function pretty(keys: string | undefined): string {
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
    <div class="ep-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="palette" role="dialog" aria-label={t("palette.title")}>
        <div class="palette-input">
          <Icon name="command" size={16} />
          <input
            aria-label={t("palette.placeholder")}
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
        </div>
        <ul role="listbox">
          <For each={list()}>
            {(a, i) => (
              <li>
                <button type="button" role="option" aria-selected={i() === sel()} classList={{ active: i() === sel() }} onMouseEnter={() => setSel(i())} onClick={() => run(a)}>
                  <span>{a.title()}</span>
                  <Show when={keysOf(a)}>{(k) => <Kbd>{pretty(k())}</Kbd>}</Show>
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
    <div onKeyDown={capture}>
      <Dialog size="lg" title={t("keymap.title")} onCancel={() => (capturing() ? setCapturing(null) : props.onClose())} actions={<Button onClick={props.onClose}>{t("common.close")}</Button>}>
        <div class="keymap-list">
          <table class="keymap">
            <tbody>
              <For each={actions()}>
                {(a) => (
                  <tr>
                    <td>{a.title()}</td>
                    <td>
                      <button type="button" class="key-button" onClick={() => setCapturing(a.id)}>
                        {capturing() === a.id ? t("keymap.press") : pretty(keysOf(a)) || "—"}
                      </button>
                    </td>
                    <td>
                      <Button variant="quiet" onClick={() => setKeys(a.id, null)}>
                        {t("keymap.reset")}
                      </Button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Dialog>
    </div>
  );
}

export function Prompt(props: { message: string; placeholder: string; action?: string; onAnswer: (value: string | null) => void }): JSX.Element {
  let input!: HTMLInputElement;
  return (
    <Dialog
      title={props.message}
      onCancel={() => props.onAnswer(null)}
      actions={
        <>
          <Button onClick={() => props.onAnswer(null)}>{t("common.cancel")}</Button>
          <Button variant="primary" onClick={() => props.onAnswer(input.value.trim() || null)}>
            {props.action ?? t("common.open")}
          </Button>
        </>
      }
    >
      <input
        class="ep-input prompt-input"
        aria-label={props.message}
        ref={input}
        data-autofocus
        placeholder={props.placeholder}
        onKeyDown={(e) => {
          if (e.key === "Enter") props.onAnswer(e.currentTarget.value.trim() || null);
        }}
      />
    </Dialog>
  );
}
