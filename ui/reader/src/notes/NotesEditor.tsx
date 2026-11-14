// The paper's notebook (P1.MD.NOTE.2–3, guideline §10.2): Markdown shown formatted while typing,
// no permanent toolbar, saved as you type; location chips (`[p. 4](loc:4/120)`) show as small
// buttons that go back to the place in the paper.

import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { EditorSelection, EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, MatchDecorator, ViewPlugin, type ViewUpdate, WidgetType, keymap, placeholder } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { t } from "@electropaper/kit";
import { type JSX, createEffect, on, onCleanup, onMount } from "solid-js";

import { goToPlace } from "../shell/frame";
import { edit, flush, incoming, notesOf, parseLoc } from "./state";

const style = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.35em", fontWeight: "600" },
  { tag: tags.heading2, fontSize: "1.18em", fontWeight: "600" },
  { tag: tags.heading3, fontSize: "1.05em", fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.quote, color: "var(--fg-muted)" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--fg-muted)" },
  { tag: tags.monospace, fontFamily: "var(--font-mono)", fontSize: "0.92em" },
  { tag: tags.processingInstruction, color: "var(--fg-muted)" },
  { tag: tags.list, color: "var(--fg-muted)" },
]);

class Chip extends WidgetType {
  constructor(
    readonly label: string,
    readonly href: string,
    readonly paper: string,
  ) {
    super();
  }
  override eq(o: Chip): boolean {
    return o.label === this.label && o.href === this.href;
  }
  toDOM(): HTMLElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "loc-chip";
    b.textContent = this.label;
    b.dataset.tip = t("notes.go_to_place");
    b.setAttribute("aria-label", `${t("notes.go_to_place")}: ${this.label}`);
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", () => {
      const loc = parseLoc(this.href);
      if (loc) goToPlace(this.paper, loc.page, loc.y, loc.annotation);
    });
    return b;
  }
  override ignoreEvent(): boolean {
    return true;
  }
}

function chips(paper: string): Extension {
  const deco = new MatchDecorator({
    regexp: /\[(p\. \d+)\]\((loc:[^)\s]+)\)/g,
    decoration: (m) => Decoration.replace({ widget: new Chip(m[1] ?? "", m[2] ?? "", paper) }),
  });
  const math = new MatchDecorator({ regexp: /\$[^$\n]+\$/g, decoration: () => Decoration.mark({ class: "cm-math" }) });
  const plugin = (d: MatchDecorator): Extension =>
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = d.createDeco(view);
        }
        update(u: ViewUpdate): void {
          this.decorations = d.updateDeco(u, this.decorations);
        }
      },
      {
        decorations: (v) => v.decorations,
        provide: (p) => EditorView.atomicRanges.of((view) => view.plugin(p)?.decorations ?? Decoration.none),
      },
    );
  return [plugin(deco), plugin(math)];
}

const MARKS = new Set(["HeaderMark", "EmphasisMark", "CodeMark", "StrikethroughMark", "QuoteMark"]);

/** Formatted while typing: on lines without the cursor, Markdown marks (#, **, `, >) are hidden
 *  and quotes get a quiet bar; the line being edited shows its marks. */
const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate): void {
      if (u.docChanged || u.selectionSet || u.viewportChanged || u.focusChanged) this.decorations = this.build(u.view);
    }
    build(view: EditorView): DecorationSet {
      const out: Range<Decoration>[] = [];
      const doc = view.state.doc;
      const active = new Set<number>();
      if (view.hasFocus) for (const r of view.state.selection.ranges) for (let l = doc.lineAt(r.from).number; l <= doc.lineAt(r.to).number; l++) active.add(l);
      for (const { from, to } of view.visibleRanges) {
        syntaxTree(view.state).iterate({
          from,
          to,
          enter: (node) => {
            const line = doc.lineAt(node.from);
            if (node.name === "Blockquote") {
              for (let l = line.number; l <= doc.lineAt(node.to).number; l++) out.push(Decoration.line({ class: "cm-quote-line" }).range(doc.line(l).from));
            }
            if (!MARKS.has(node.name) || active.has(line.number)) return;
            // A heading or quote mark takes the space after it too.
            const end = (node.name === "HeaderMark" || node.name === "QuoteMark") && doc.sliceString(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
            if (end > node.from) out.push(Decoration.replace({}).range(node.from, end));
          },
        });
      }
      return Decoration.set(out, true);
    }
  },
  { decorations: (v) => v.decorations },
);

/** Wrap the selection in a Markdown mark (Ctrl+B, Ctrl+I). */
function wrap(mark: string) {
  return (view: EditorView): boolean => {
    view.dispatch(
      view.state.changeByRange((r) => ({
        changes: [
          { from: r.from, insert: mark },
          { from: r.to, insert: mark },
        ],
        range: EditorSelection.range(r.from + mark.length, r.to + mark.length),
      })),
    );
    return true;
  };
}

export function NotesEditor(props: { paper: string }): JSX.Element {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: notesOf(props.paper)?.text ?? "",
        extensions: [
          history(),
          keymap.of([{ key: "Mod-b", run: wrap("**") }, { key: "Mod-i", run: wrap("*") }, ...historyKeymap, ...defaultKeymap]),
          markdown(),
          syntaxHighlighting(style),
          livePreview,
          EditorView.lineWrapping,
          placeholder(t("notes.empty")),
          chips(props.paper),
          EditorView.contentAttributes.of({ "aria-label": t("notes.editor"), spellcheck: "true" }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) edit(props.paper, u.state.doc.toString());
          }),
        ],
      }),
    });
    onCleanup(() => {
      void flush(props.paper);
      view?.destroy();
    });
  });

  // Text loaded or changed elsewhere (another window) while nothing is being typed here.
  createEffect(() => {
    const e = notesOf(props.paper);
    if (!view || !e || e.dirty) return;
    const doc = view.state.doc.toString();
    if (doc !== e.text) view.dispatch({ changes: { from: 0, to: doc.length, insert: e.text } });
  });

  // "Add to notes": insert at the cursor (or at the end), then show it.
  createEffect(
    on(incoming, (q) => {
      if (!q || q.paper !== props.paper || !view) return;
      const sel = view.state.selection.main;
      const at = view.hasFocus ? sel.head : view.state.doc.length;
      const before = at > 0 && view.state.doc.sliceString(at - 1, at) !== "\n" ? "\n\n" : "";
      const insert = before + q.text;
      view.dispatch({ changes: { from: at, insert }, selection: { anchor: at + insert.length }, scrollIntoView: true });
      view.focus();
    }),
  );

  return <div class="notes-editor" ref={host} />;
}
