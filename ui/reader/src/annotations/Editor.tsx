// The card next to a picked annotation (P1.M3.UI.2–4, guideline §10.3): colour, category,
// comment, replies and resolve, text size or line width, add to notes, delete. Changes are stored
// as soon as a field is left.

import type { Annotation, AnnotationKind, Category } from "@electropaper/bindings";
import { Button, Card, IconButton, type IconName, type MessageKey, Segmented, Swatches, t } from "@electropaper/kit";
import { For, type JSX, Show, createSignal, onMount } from "solid-js";

import { quoteToNotes } from "../notes/state";
import { newId } from "./geometry";
import { CATEGORIES, COLORS, INK_COLORS, add, change, colorNames, draft, isMarkup, picked, remove, repliesOf, setPending, setPicked } from "./state";

const CARD_W = 320;
const WIDTHS = ["0.75", "1.5", "3", "5"];
const SIZES = [9, 11, 12, 14, 18, 24];

export const KIND_ICON: Record<AnnotationKind, IconName> = {
  highlight: "highlighter",
  underline: "underline",
  strike_out: "strikethrough",
  squiggly: "spline",
  note: "message-circle",
  free_text: "type",
  ink: "pen-line",
  rectangle: "square",
  ellipse: "circle",
  line: "minus",
  arrow: "move-up-right",
};

function place(a: Annotation, scale: number, pageWidth: number, pageHeight: number): JSX.CSSProperties {
  const right = a.rect.x1 * scale + 12;
  const left = right + CARD_W > pageWidth ? Math.max(4, a.rect.x0 * scale - CARD_W - 12) : right;
  return {
    position: "absolute",
    left: `${Math.max(4, Math.min(left, pageWidth - CARD_W - 4))}px`,
    top: `${Math.max(4, Math.min(a.rect.y0 * scale, pageHeight - 340))}px`,
    width: `${CARD_W}px`,
  };
}

export const kindLabel = (k: Annotation["kind"]): string => t(`annot.kind.${k}` as MessageKey);
export const categoryLabel = (c: Category): string => t(`annot.category.${c}` as MessageKey);

export function when(ms: number): string {
  return new Date(ms).toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

const stop = {
  onPointerDown: (e: PointerEvent) => e.stopPropagation(),
  onClick: (e: MouseEvent) => e.stopPropagation(),
};

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
  const markColors = (): boolean => isMarkup(a().kind) || a().kind === "note";

  return (
    <div class="annot-card-host" style={place(a(), props.scale, props.pageWidth, props.pageHeight)} {...stop}>
      <Card
        class="annot-card"
        title={kindLabel(a().kind)}
        icon={KIND_ICON[a().kind]}
        meta={`${a().author ?? ""} · ${when(a().modified_ms)}`}
        onClose={close}
        closeLabel={t("common.close")}
        footer={
          <>
            <label class="check">
              <input type="checkbox" checked={a().resolved ?? false} onChange={(e) => save({ resolved: e.currentTarget.checked })} />
              {t("annot.resolved")}
            </label>
            <span class="ep-spacer" />
            <IconButton icon="notebook-pen" size="sm" label={t("notes.add_to_notes")} onClick={() => void quoteToNotes(props.paper, { text: a().text ?? a().body ?? "", page: a().page, y: a().rect.y0, annotation: a().id })} />
            <IconButton icon="trash" size="sm" label={t("annot.delete")} shortcut="Delete" onClick={() => void remove(props.paper, a().id)} />
          </>
        }
      >
        <div
          class="annot-card-inner"
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              close();
            }
          }}
        >
          <Show when={a().text}>
            <blockquote class="quote">{a().text}</blockquote>
          </Show>
          <div class="row">
            <Swatches label={t("annot.color")} colors={markColors() ? COLORS : INK_COLORS} names={colorNames} value={a().color} onPick={(color) => save({ color })} />
            <span class="ep-spacer" />
            <select class="ep-select" aria-label={t("annot.category")} value={a().category ?? ""} onChange={(e) => save({ category: (e.currentTarget.value || null) as Category | null })}>
              <option value="">{t("annot.no_category")}</option>
              <For each={CATEGORIES}>{(c) => <option value={c}>{categoryLabel(c)}</option>}</For>
            </select>
          </div>
          <Show when={a().kind === "free_text"}>
            <select class="ep-select" aria-label={t("annot.font_size")} value={a().font_size ?? 12} onChange={(e) => save({ font_size: Number(e.currentTarget.value) })}>
              <For each={SIZES}>{(s) => <option value={s}>{`${s} pt`}</option>}</For>
            </select>
          </Show>
          <Show when={["ink", "rectangle", "ellipse", "line", "arrow"].includes(a().kind)}>
            <Segmented label={t("annot.width")} value={String(a().width ?? 1.5)} options={WIDTHS.map((w) => ({ value: w, label: `${w} pt` }))} onChange={(w) => save({ width: Number(w) })} />
          </Show>
          <textarea
            ref={body}
            class="ep-input body"
            rows={a().kind === "free_text" ? 3 : 2}
            aria-label={a().kind === "free_text" ? t("annot.text_placeholder") : t("annot.comment_placeholder")}
            placeholder={a().kind === "free_text" ? t("annot.text_placeholder") : t("annot.comment_placeholder")}
            value={a().body ?? ""}
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
                    <div class="reply-meta">
                      <span>
                        {r.author ?? ""} · {when(r.created_ms)}
                      </span>
                      <IconButton icon="x" size="sm" label={t("annot.delete_reply")} onClick={() => void remove(props.paper, r.id)} />
                    </div>
                    <div class="reply-body">{r.body}</div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <div class="row reply-row">
            <input
              class="ep-input"
              aria-label={t("annot.reply_placeholder")}
              placeholder={t("annot.reply_placeholder")}
              value={reply()}
              onInput={(e) => setReply(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void sendReply();
              }}
            />
            <Button disabled={!reply().trim()} onClick={() => void sendReply()}>
              {t("annot.reply")}
            </Button>
          </div>
        </div>
      </Card>
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
    <div class="annot-card-host" style={place(props.annotation, props.scale, props.pageWidth, props.pageHeight)} {...stop}>
      <Card
        class="annot-card pending"
        title={kindLabel(props.annotation.kind)}
        icon={KIND_ICON[props.annotation.kind]}
        onClose={() => setPending(null)}
        closeLabel={t("common.cancel")}
        footer={
          <>
            <span class="muted small">{t("annot.save_hint")}</span>
            <span class="ep-spacer" />
            <Button onClick={() => setPending(null)}>{t("common.cancel")}</Button>
            <Button variant="primary" onClick={() => void save()}>
              {t("annot.save")}
            </Button>
          </>
        }
      >
        <Show when={props.annotation.text}>
          <blockquote class="quote">{props.annotation.text}</blockquote>
        </Show>
        <textarea
          ref={box}
          class="ep-input body"
          rows={3}
          aria-label={props.annotation.kind === "free_text" ? t("annot.text_placeholder") : t("annot.comment_placeholder")}
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
      </Card>
    </div>
  );
}
