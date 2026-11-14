// The card next to a picked annotation (P1.M3.UI.2–4): colour, category, comment, replies and
// resolve, text size or line width, delete. Changes are stored as soon as a field is left.

import type { Annotation, Category } from "@electropaper/bindings";
import { type MessageKey, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onMount } from "solid-js";

import { newId } from "./geometry";
import { CATEGORIES, COLORS, INK_COLORS, add, change, draft, isMarkup, picked, remove, repliesOf, setPending, setPicked } from "./state";

const CARD_W = 300;
const WIDTHS = [0.75, 1.5, 3, 5];
const SIZES = [9, 11, 12, 14, 18, 24];

function place(a: Annotation, scale: number, pageWidth: number, pageHeight: number): JSX.CSSProperties {
  const right = a.rect.x1 * scale + 10;
  const left = right + CARD_W > pageWidth ? Math.max(4, a.rect.x0 * scale - CARD_W - 10) : right;
  return { left: `${Math.max(4, Math.min(left, pageWidth - CARD_W - 4))}px`, top: `${Math.max(4, Math.min(a.rect.y0 * scale, pageHeight - 320))}px`, width: `${CARD_W}px` };
}

export const kindLabel = (k: Annotation["kind"]): string => t(`annot.kind.${k}` as MessageKey);
export const categoryLabel = (c: Category): string => t(`annot.category.${c}` as MessageKey);

function when(ms: number): string {
  return new Date(ms).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" });
}

function Swatches(props: { colors: readonly string[]; value: string; onPick: (c: string) => void }): JSX.Element {
  return (
    <div class="swatches" role="radiogroup" aria-label={t("annot.color")}>
      <For each={props.colors}>
        {(c) => (
          <button
            type="button"
            class="swatch"
            role="radio"
            aria-checked={props.value === c}
            aria-label={c}
            style={{ background: c }}
            onClick={() => props.onPick(c)}
          />
        )}
      </For>
    </div>
  );
}

export function Editor(props: { paper: string; annotation: Annotation; scale: number; pageWidth: number; pageHeight: number }): JSX.Element {
  const a = (): Annotation => props.annotation;
  const [reply, setReply] = createSignal("");
  let body!: HTMLTextAreaElement;
  const save = (patch: Partial<Annotation>): void => {
    void change(props.paper, { ...a(), ...patch });
  };
  const close = (): void => {
    setPicked(null);
  };
  const replies = (): Annotation[] => repliesOf(props.paper, a().id);
  const sendReply = async (): Promise<void> => {
    const text = reply().trim();
    if (!text) return;
    const r = draft("note", a().page, { id: newId(), parent: a().id, body: text, rect: { ...a().rect, x1: a().rect.x0 + 20, y1: a().rect.y0 + 20 }, color: a().color, category: null });
    if (await add(props.paper, [r])) setReply("");
  };
  onMount(() => {
    if (picked()?.focus) body.focus();
  });
  const markup = (): boolean => isMarkup(a().kind) || a().kind === "note";

  return (
    <div
      class="annot-card"
      style={place(a(), props.scale, props.pageWidth, props.pageHeight)}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          close();
        }
      }}
    >
      <header>
        <strong>{kindLabel(a().kind)}</strong>
        <span class="muted small">
          {a().author ?? ""} · {when(a().modified_ms)}
        </span>
        <button type="button" class="icon" aria-label={t("common.close")} onClick={close}>
          ×
        </button>
      </header>
      <Show when={a().text}>
        <blockquote class="quote">{a().text}</blockquote>
      </Show>
      <div class="row">
        <Swatches colors={markup() ? COLORS : INK_COLORS} value={a().color} onPick={(color) => save({ color })} />
        <select class="ep-select" aria-label={t("annot.category")} value={a().category ?? ""} onChange={(e) => save({ category: (e.currentTarget.value || null) as Category | null })}>
          <option value="">{t("annot.no_category")}</option>
          <For each={CATEGORIES}>{(c) => <option value={c}>{categoryLabel(c)}</option>}</For>
        </select>
      </div>
      <Show when={a().kind === "free_text"}>
        <label class="field">
          {t("annot.font_size")}
          <select class="ep-select" value={a().font_size ?? 12} onChange={(e) => save({ font_size: Number(e.currentTarget.value) })}>
            <For each={SIZES}>{(s) => <option value={s}>{s}</option>}</For>
          </select>
        </label>
      </Show>
      <Show when={["ink", "rectangle", "ellipse", "line", "arrow"].includes(a().kind)}>
        <label class="field">
          {t("annot.width")}
          <select class="ep-select" value={a().width ?? 1.5} onChange={(e) => save({ width: Number(e.currentTarget.value) })}>
            <For each={WIDTHS}>{(w) => <option value={w}>{w}</option>}</For>
          </select>
        </label>
      </Show>
      <textarea
        ref={body}
        class="ep-input body"
        rows={a().kind === "free_text" ? 3 : 2}
        placeholder={a().kind === "free_text" ? t("annot.text_placeholder") : t("annot.comment_placeholder")}
        value={a().body}
        onBlur={(e) => {
          const v = e.currentTarget.value;
          if (v !== (a().body ?? "")) save({ body: v });
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.ctrlKey) e.currentTarget.blur();
        }}
      />
      <Show when={replies().length > 0}>
        <ul class="replies">
          <For each={replies()}>
            {(r) => (
              <li>
                <div class="muted small">
                  {r.author ?? ""} · {when(r.created_ms)}
                  <button type="button" class="icon small" aria-label={t("annot.delete")} onClick={() => void remove(props.paper, r.id)}>
                    ×
                  </button>
                </div>
                <div class="reply-body">{r.body}</div>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <div class="row">
        <input
          class="ep-input"
          placeholder={t("annot.reply_placeholder")}
          value={reply()}
          onInput={(e) => setReply(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void sendReply();
          }}
        />
        <button type="button" class="ep-button" disabled={!reply().trim()} onClick={() => void sendReply()}>
          {t("annot.reply")}
        </button>
      </div>
      <footer class="row">
        <label class="check">
          <input type="checkbox" checked={a().resolved ?? false} onChange={(e) => save({ resolved: e.currentTarget.checked })} />
          {t("annot.resolved")}
        </label>
        <span class="spacer" />
        <button type="button" class="ep-button danger" onClick={() => void remove(props.paper, a().id)}>
          {t("annot.delete")}
        </button>
      </footer>
    </div>
  );
}

/** The card for a new note, free text or comment: nothing is added until it is saved. */
export function PendingEditor(props: { paper: string; annotation: Annotation; scale: number; pageWidth: number; pageHeight: number }): JSX.Element {
  const [text, setText] = createSignal(props.annotation.body ?? "");
  let box!: HTMLTextAreaElement;
  const needsText = (): boolean => props.annotation.kind === "note" || props.annotation.kind === "free_text";
  const save = async (): Promise<void> => {
    const body = text().trim();
    if (needsText() && !body) return void setPending(null);
    const a = { ...props.annotation, body };
    setPending(null);
    if (await add(props.paper, [a])) setPicked({ paper: props.paper, id: a.id, edit: false });
  };
  onMount(() => box.focus());
  return (
    <div
      class="annot-card pending"
      style={place(props.annotation, props.scale, props.pageWidth, props.pageHeight)}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
    >
      <header>
        <strong>{kindLabel(props.annotation.kind)}</strong>
      </header>
      <Show when={props.annotation.text}>
        <blockquote class="quote">{props.annotation.text}</blockquote>
      </Show>
      <textarea
        ref={box}
        class="ep-input body"
        rows={3}
        placeholder={props.annotation.kind === "free_text" ? t("annot.text_placeholder") : t("annot.comment_placeholder")}
        value={text()}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.ctrlKey) void save();
          if (e.key === "Escape") {
            e.stopPropagation();
            setPending(null);
          }
        }}
      />
      <footer class="row">
        <span class="muted small">{t("annot.save_hint")}</span>
        <span class="spacer" />
        <button type="button" class="ep-button" onClick={() => setPending(null)}>
          {t("common.cancel")}
        </button>
        <button type="button" class="ep-button" data-variant="primary" onClick={() => void save()}>
          {t("annot.save")}
        </button>
      </footer>
    </div>
  );
}
