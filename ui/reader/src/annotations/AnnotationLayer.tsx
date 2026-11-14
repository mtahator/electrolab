// The annotation layer of one page (P1.M3.UI.1–5): every annotation drawn as SVG in page space,
// picking, moving and resizing in the select tool, and the drawing tools. Each tool is a small
// state machine (idle → dragging → committing) kept in this component; nothing is shared between
// tools except the options in `state.ts`.

import type { Annotation, PagePoint, PageRect } from "@electropaper/bindings";
import { t } from "@electropaper/kit";
import { For, type JSX, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";

import { Editor, PendingEditor } from "./Editor";
import { boundsOf, hits, simplify } from "./geometry";
import { add, annotationsOf, change, draft, isMarkup, marksHidden, pending, picked, remove, repliesOf, setPending, setPicked, setTool, tool } from "./state";

export interface LayerProps {
  paper: string;
  page: number;
  /** CSS pixels per point. */
  scale: number;
  width: number;
  height: number;
}

export type Drag =
  | { kind: "draw"; start: PagePoint; cur: PagePoint; points: PagePoint[] }
  | { kind: "erase"; ids: Set<string> }
  | { kind: "move"; id: string; start: PagePoint; dx: number; dy: number; moved: boolean }
  | { kind: "resize"; id: string; handle: number; start: PagePoint; cur: PagePoint };

const DRAW_TOOLS = new Set(["note", "free_text", "ink", "rectangle", "ellipse", "line", "arrow", "eraser"]);
const MOVABLE = new Set(["note", "free_text", "ink", "rectangle", "ellipse", "line", "arrow"]);

const f = (v: number): string => v.toFixed(2);

/** Geometry of `a` with a move or resize applied (for the live preview and the committed change). */
export function transformed(a: Annotation, drag: Drag | null): Annotation {
  if (!drag || !("id" in drag) || drag.id !== a.id) return a;
  if (drag.kind === "move") {
    const { dx, dy } = drag;
    const mv = (p: PagePoint): PagePoint => ({ x: p.x + dx, y: p.y + dy });
    return {
      ...a,
      rect: { ...a.rect, x0: a.rect.x0 + dx, x1: a.rect.x1 + dx, y0: a.rect.y0 + dy, y1: a.rect.y1 + dy },
      paths: a.paths?.map((s) => s.map(mv)),
      line: a.line ? a.line.map(mv) : a.line,
    };
  }
  // Resize: handles 0–3 are the corners (top-left, top-right, bottom-left, bottom-right);
  // for lines, 0 is the start and 1 the end.
  const { handle, cur } = drag;
  if ((a.kind === "line" || a.kind === "arrow") && a.line) {
    const line = [...a.line];
    line[handle === 0 ? 0 : 1] = cur;
    return { ...a, line };
  }
  const r = { ...a.rect };
  if (handle === 0 || handle === 2) r.x0 = cur.x;
  else r.x1 = cur.x;
  if (handle === 0 || handle === 1) r.y0 = cur.y;
  else r.y1 = cur.y;
  const norm: PageRect = { page: r.page, x0: Math.min(r.x0, r.x1), y0: Math.min(r.y0, r.y1), x1: Math.max(r.x0, r.x1), y1: Math.max(r.y0, r.y1) };
  return { ...a, rect: norm };
}

/** One annotation as SVG (page space). */
/** The author's initial (or a dot when unknown). */
function initial(author: string | null | undefined): string {
  const c = (author ?? "").trim().charAt(0);
  return c ? c.toUpperCase() : "•";
}

/** A comment pin: a round bubble whose bottom-left corner points at the spot, with a short label
 *  (author initial or the number of comments in the thread). */
export function Pin(props: { x: number; y: number; size: number; color: string; label?: string }): JSX.Element {
  const r = (): number => props.size / 2;
  return (
    <g class="pin">
      <path
        class="pin-body"
        d={`M${f(props.x)} ${f(props.y + props.size)} V${f(props.y + r())} A${f(r())} ${f(r())} 0 1 1 ${f(props.x + r())} ${f(props.y + props.size)} Z`}
        fill={props.color}
      />
      <Show when={props.label}>
        <text class="pin-text" x={props.x + r()} y={props.y + r()} style={{ "font-size": `${props.size * 0.52}px` }}>
          {props.label ?? ""}
        </text>
      </Show>
    </g>
  );
}

function Shape(props: { a: Annotation; picked: boolean; fading: boolean; replies: number }): JSX.Element {
  const a = (): Annotation => props.a;
  const w = (): number => a().width ?? 1;
  // Resolved notes step back (guideline §10.1).
  const op = (): number => (props.fading ? 0.25 : a().kind === "note" && a().resolved ? 0.4 : (a().opacity ?? 1));
  const quads = (): Annotation["quads"] => a().quads ?? [];
  const markupLine = (q: NonNullable<Annotation["quads"]>[number], at: "bottom" | "middle"): { x1: number; y1: number; x2: number; y2: number; t: number } => {
    const [ul, ur, ll, lr] = q.p;
    const h = Math.hypot((ul?.x ?? 0) - (ll?.x ?? 0), (ul?.y ?? 0) - (ll?.y ?? 0));
    const tk = Math.max(0.6, h / 14);
    if (at === "bottom") return { x1: (ll?.x ?? 0), y1: (ll?.y ?? 0) - tk, x2: (lr?.x ?? 0), y2: (lr?.y ?? 0) - tk, t: tk };
    return { x1: ((ul?.x ?? 0) + (ll?.x ?? 0)) / 2, y1: ((ul?.y ?? 0) + (ll?.y ?? 0)) / 2, x2: ((ur?.x ?? 0) + (lr?.x ?? 0)) / 2, y2: ((ur?.y ?? 0) + (lr?.y ?? 0)) / 2, t: tk };
  };
  const zigzag = (q: NonNullable<Annotation["quads"]>[number]): string => {
    const { x1, y1, x2, y2, t: tk } = markupLine(q, "bottom");
    const len = Math.max(0.1, Math.hypot(x2 - x1, y2 - y1));
    const dx = (x2 - x1) / len;
    const dy = (y2 - y1) / len;
    const amp = tk * 1.4;
    const pts = [`${f(x1)},${f(y1)}`];
    for (let s = amp * 2, up = true; s <= len; s += amp * 2, up = !up) {
      const o = up ? -amp : 0;
      pts.push(`${f(x1 + dx * s - dy * o)},${f(y1 + dy * s + dx * o)}`);
    }
    return pts.join(" ");
  };
  return (
    <g data-annotation={a().id} data-kind={a().kind} opacity={op()} classList={{ picked: props.picked }}>
      {(() => {
        const k = a().kind;
        if (k === "highlight")
          return <For each={quads()}>{(q) => <polygon class="hl" fill={a().color} points={[q.p[0], q.p[1], q.p[3], q.p[2]].map((p) => `${f(p?.x ?? 0)},${f(p?.y ?? 0)}`).join(" ")} />}</For>;
        if (k === "underline" || k === "strike_out")
          return (
            <For each={quads()}>
              {(q) => {
                const l = markupLine(q, k === "underline" ? "bottom" : "middle");
                return <line x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2} stroke={a().color} stroke-width={l.t} />;
              }}
            </For>
          );
        if (k === "squiggly") return <For each={quads()}>{(q) => <polyline points={zigzag(q)} fill="none" stroke={a().color} stroke-width={Math.max(0.5, markupLine(q, "bottom").t * 0.7)} />}</For>;
        if (k === "ink")
          return <For each={a().paths ?? []}>{(s) => <polyline points={s.map((p) => `${f(p.x)},${f(p.y)}`).join(" ")} fill="none" stroke={a().color} stroke-width={w()} stroke-linecap="round" stroke-linejoin="round" />}</For>;
        if (k === "rectangle") {
          const r = a().rect;
          return <rect x={r.x0 + w() / 2} y={r.y0 + w() / 2} width={Math.max(0, r.x1 - r.x0 - w())} height={Math.max(0, r.y1 - r.y0 - w())} fill="none" stroke={a().color} stroke-width={w()} />;
        }
        if (k === "ellipse") {
          const r = a().rect;
          return <ellipse cx={(r.x0 + r.x1) / 2} cy={(r.y0 + r.y1) / 2} rx={Math.max(0.1, (r.x1 - r.x0 - w()) / 2)} ry={Math.max(0.1, (r.y1 - r.y0 - w()) / 2)} fill="none" stroke={a().color} stroke-width={w()} />;
        }
        if (k === "line" || k === "arrow") {
          const [s, e] = a().line ?? [];
          if (!s || !e) return null;
          const head = (): string => {
            const len = Math.max(0.01, Math.hypot(e.x - s.x, e.y - s.y));
            const ux = (e.x - s.x) / len;
            const uy = (e.y - s.y) / len;
            const hl = 3 + w() * 3;
            const hw = 1.5 + w() * 1.5;
            return `${f(e.x - hl * ux + hw * uy)},${f(e.y - hl * uy - hw * ux)} ${f(e.x)},${f(e.y)} ${f(e.x - hl * ux - hw * uy)},${f(e.y - hl * uy + hw * ux)}`;
          };
          return (
            <>
              <line x1={s.x} y1={s.y} x2={e.x} y2={e.y} stroke={a().color} stroke-width={w()} stroke-linecap="round" />
              <Show when={k === "arrow"}>
                <polyline points={head()} fill="none" stroke={a().color} stroke-width={w()} stroke-linecap="round" stroke-linejoin="round" />
              </Show>
            </>
          );
        }
        if (k === "free_text") {
          const r = a().rect;
          return (
            <foreignObject x={r.x0} y={r.y0} width={Math.max(1, r.x1 - r.x0)} height={Math.max(1, r.y1 - r.y0)}>
              <div class="free-text" style={{ color: a().color, "font-size": `${a().font_size ?? 12}px` }}>
                {a().body}
              </div>
            </foreignObject>
          );
        }
        if (k === "note") {
          const r = a().rect;
          return (
            // A comment pin (guideline §10.1): small, round, pointing at its spot.
            <Pin x={r.x0 + 2} y={r.y0 + 2} size={16} color={a().color} label={props.replies > 0 ? String(props.replies + 1) : initial(a().author)} />
          );
        }
        return null;
      })()}
    </g>
  );
}

export function AnnotationLayer(props: LayerProps): JSX.Element {
  let host!: HTMLDivElement;
  const [drag, setDrag] = createSignal<Drag | null>(null);
  // Ctrl+H hides every mark for clean reading.
  const all = createMemo(() => (marksHidden() ? [] : (annotationsOf(props.paper)?.annotations ?? []).filter((a) => a.page === props.page)));
  // Replies are listed in their note's card, not drawn.
  const shown = createMemo(() => all().filter((a) => !a.parent));
  const pickedHere = createMemo(() => {
    const p = picked();
    return p && p.paper === props.paper ? all().find((a) => a.id === p.id) : undefined;
  });
  const pendingHere = createMemo(() => {
    const p = pending();
    return p && p.paper === props.paper && p.annotation.page === props.page ? p.annotation : undefined;
  });
  const drawing = (): boolean => DRAW_TOOLS.has(tool());

  const pt = (e: PointerEvent | MouseEvent): PagePoint => {
    const r = host.getBoundingClientRect();
    return { x: (e.clientX - r.left) / props.scale, y: (e.clientY - r.top) / props.scale };
  };

  // Markup (highlights etc.) lies under the text layer, which takes the clicks: a click that
  // selects no text picks the mark under it.
  onMount(() => {
    const page = host.parentElement;
    if (!page) return;
    const onClick = (e: MouseEvent): void => {
      if (tool() !== "select" || drawing()) return;
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) return;
      if ((e.target as HTMLElement).closest(".annot-card, .annot-proxy, .link")) return;
      const p = pt(e);
      const hit = [...shown()].reverse().find((a) => isMarkup(a.kind) && hits(a, p.x, p.y));
      if (hit) setPicked({ paper: props.paper, id: hit.id, edit: true });
      else if (picked()?.paper === props.paper && !(e.target as HTMLElement).closest(".annot-layer")) setPicked(null);
    };
    page.addEventListener("click", onClick);
    onCleanup(() => page.removeEventListener("click", onClick));
  });

  // A drag is followed on the window, so it continues outside the page and over other layers.
  const follow = (): void => {
    const move = (e: PointerEvent): void => onMove(e);
    const up = (e: PointerEvent): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      onUp(e);
    };
    const cancel = (): void => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      setDrag(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
  };

  // ---- drawing tools -------------------------------------------------------------------------
  const onDown = (e: PointerEvent): void => {
    if (e.button !== 0) return;
    e.preventDefault();
    follow();
    const p = pt(e);
    if (tool() === "eraser") {
      const ids = new Set<string>();
      const hit = [...shown()].reverse().find((a) => hits(a, p.x, p.y, 4));
      if (hit) ids.add(hit.id);
      setDrag({ kind: "erase", ids });
      return;
    }
    setDrag({ kind: "draw", start: p, cur: p, points: [p] });
  };

  const onMove = (e: PointerEvent): void => {
    const d = drag();
    if (!d) return;
    const p = pt(e);
    if (d.kind === "erase") {
      const hit = shown().find((a) => !d.ids.has(a.id) && hits(a, p.x, p.y, 4));
      if (hit) setDrag({ kind: "erase", ids: new Set([...d.ids, hit.id]) });
    } else if (d.kind === "draw") {
      // Every pointer event at full rate (coalesced ones too); simplified on commit.
      const coalesced = (e as PointerEvent & { getCoalescedEvents?: () => PointerEvent[] }).getCoalescedEvents?.() ?? [];
      const evs = coalesced.length > 0 ? coalesced : [e];
      const more = tool() === "ink" ? evs.map(pt) : [];
      setDrag({ ...d, cur: p, points: tool() === "ink" ? [...d.points, ...more] : d.points });
    } else if (d.kind === "move") {
      const dx = p.x - d.start.x;
      const dy = p.y - d.start.y;
      setDrag({ ...d, dx, dy, moved: d.moved || Math.hypot(dx, dy) * props.scale > 3 });
    } else {
      setDrag({ ...d, cur: p });
    }
  };

  const onUp = (_e: PointerEvent): void => {
    const d = drag();
    setDrag(null);
    if (!d) return;
    if (d.kind === "erase") {
      if (d.ids.size > 0) void (async () => {
        for (const id of d.ids) await remove(props.paper, id);
      })();
      return;
    }
    if (d.kind === "move" || d.kind === "resize") {
      const a = all().find((x) => x.id === d.id);
      if (!a) return;
      if (d.kind === "move" && !d.moved) {
        setPicked({ paper: props.paper, id: a.id, edit: true });
        return;
      }
      void change(props.paper, transformed(a, d));
      return;
    }
    commitDraw(d);
  };

  const commitDraw = (d: Extract<Drag, { kind: "draw" }>): void => {
    const k = tool();
    const page = props.page;
    const { start, cur } = d;
    const dragged = Math.hypot(cur.x - start.x, cur.y - start.y) * props.scale > 4;
    const box = (dw: number, dh: number): PageRect =>
      dragged ? { page, x0: Math.min(start.x, cur.x), y0: Math.min(start.y, cur.y), x1: Math.max(start.x, cur.x), y1: Math.max(start.y, cur.y) } : { page, x0: start.x, y0: start.y, x1: start.x + dw, y1: start.y + dh };
    if (k === "note") {
      setPending({ paper: props.paper, annotation: draft("note", page, { rect: { page, x0: start.x - 10, y0: start.y - 10, x1: start.x + 10, y1: start.y + 10 } }) });
      setTool("select");
    } else if (k === "free_text") {
      setPending({ paper: props.paper, annotation: draft("free_text", page, { rect: box(200, 40) }) });
      setTool("select");
    } else if (k === "ink") {
      const pts = simplify(d.points.length > 1 ? d.points : [start, { x: start.x + 0.5, y: start.y }]);
      void addOne(draft("ink", page, { paths: [pts], rect: boundsOf(page, pts, 2) }));
    } else if (k === "rectangle" || k === "ellipse") {
      void addOne(draft(k, page, { rect: box(80, 50) }));
    } else if (k === "line" || k === "arrow") {
      const end = dragged ? cur : { x: start.x + 60, y: start.y };
      void addOne(draft(k, page, { line: [start, end], rect: boundsOf(page, [start, end], 4) }));
    }
  };

  const addOne = async (a: Annotation): Promise<void> => {
    if (await add(props.paper, [a])) setPicked({ paper: props.paper, id: a.id, edit: false });
  };

  // ---- select tool: move and resize ------------------------------------------------------------
  const startMove = (e: PointerEvent, a: Annotation): void => {
    if (tool() !== "select" || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    follow();
    setPicked({ paper: props.paper, id: a.id, edit: picked()?.id === a.id ? (picked()?.edit ?? false) : false });
    if (MOVABLE.has(a.kind)) setDrag({ kind: "move", id: a.id, start: pt(e), dx: 0, dy: 0, moved: false });
    else setPicked({ paper: props.paper, id: a.id, edit: true });
  };

  const startResize = (e: PointerEvent, a: Annotation, handle: number): void => {
    e.preventDefault();
    e.stopPropagation();
    follow();
    setDrag({ kind: "resize", id: a.id, handle, start: pt(e), cur: pt(e) });
  };

  const handles = (a: Annotation): PagePoint[] => {
    if ((a.kind === "line" || a.kind === "arrow") && a.line) return a.line;
    if (a.kind === "rectangle" || a.kind === "ellipse" || a.kind === "free_text") {
      const r = a.rect;
      return [
        { x: r.x0, y: r.y0 },
        { x: r.x1, y: r.y0 },
        { x: r.x0, y: r.y1 },
        { x: r.x1, y: r.y1 },
      ];
    }
    return [];
  };

  const preview = (): JSX.Element => {
    const d = drag();
    if (!d || d.kind !== "draw") return null;
    const k = tool();
    const { start: s, cur: c } = d;
    if (k === "ink") return <polyline points={d.points.map((p) => `${f(p.x)},${f(p.y)}`).join(" ")} fill="none" stroke="currentColor" class="draft" stroke-width="1.5" stroke-linecap="round" />;
    if (k === "line" || k === "arrow") return <line x1={s.x} y1={s.y} x2={c.x} y2={c.y} class="draft" stroke="currentColor" stroke-width="1.5" />;
    if (k === "ellipse") return <ellipse cx={(s.x + c.x) / 2} cy={(s.y + c.y) / 2} rx={Math.abs(c.x - s.x) / 2} ry={Math.abs(c.y - s.y) / 2} class="draft" fill="none" stroke="currentColor" />;
    if (k === "rectangle" || k === "free_text") return <rect x={Math.min(s.x, c.x)} y={Math.min(s.y, c.y)} width={Math.abs(c.x - s.x)} height={Math.abs(c.y - s.y)} class="draft" fill="none" stroke="currentColor" stroke-dasharray={k === "free_text" ? "3 2" : undefined} />;
    return null;
  };

  const vw = (): number => props.width / props.scale;
  const vh = (): number => props.height / props.scale;
  const fading = (id: string): boolean => {
    const d = drag();
    return !!d && d.kind === "erase" && d.ids.has(id);
  };

  return (
    <div class="annot-host" ref={host} classList={{ drawing: drawing(), [`tool-${tool()}`]: true }} onPointerDown={(e) => drawing() && onDown(e)}>
      <svg class="annot-layer" width={props.width} height={props.height} viewBox={`0 0 ${vw()} ${vh()}`} aria-hidden="true">
        <For each={shown()}>{(a) => <Shape a={transformed(a, drag())} picked={pickedHere()?.id === a.id} fading={fading(a.id)} replies={repliesOf(props.paper, a.id).length} />}</For>
        <Show when={pendingHere()}>{(a) => <Shape a={a()} picked={true} fading={false} replies={0} />}</Show>
        {preview()}
        {/* A small pin on marks that carry a comment, so comments are visible without opening. */}
        <For each={shown().filter((a) => isMarkup(a.kind) && (a.body || repliesOf(props.paper, a.id).length > 0))}>
          {(a) => {
            const q = (a.quads ?? [])[(a.quads ?? []).length - 1];
            const x = q ? Math.max(...q.p.map((p) => p.x)) + 1 : a.rect.x1;
            const y = q ? Math.min(...q.p.map((p) => p.y)) - 9 : a.rect.y0 - 9;
            const n = repliesOf(props.paper, a.id).length + (a.body ? 1 : 0);
            return <Pin x={x} y={y} size={10} color={a.color} label={n > 1 ? String(n) : undefined} />;
          }}
        </For>
        {/* Click targets for drawn annotations (text marks are picked through the page click). */}
        <Show when={!drawing()}>
          <For each={shown().filter((a) => !isMarkup(a.kind))}>
            {(a) => {
              const g = (): Annotation => transformed(a, drag());
              const common = { class: "annot-proxy", onPointerDown: (e: PointerEvent) => startMove(e, a) };
              return (() => {
                const x = g();
                const pad = Math.max(4, (x.width ?? 1) + 3);
                if (x.kind === "ink") return <For each={x.paths ?? []}>{(s) => <polyline {...common} points={s.map((p) => `${f(p.x)},${f(p.y)}`).join(" ")} fill="none" stroke="transparent" stroke-width={pad * 2} />}</For>;
                if ((x.kind === "line" || x.kind === "arrow") && x.line) {
                  const [s, e] = x.line;
                  return <line {...common} x1={s?.x} y1={s?.y} x2={e?.x} y2={e?.y} stroke="transparent" stroke-width={pad * 2} />;
                }
                const r = x.rect;
                if (x.kind === "rectangle" || x.kind === "ellipse") return <rect {...common} x={r.x0} y={r.y0} width={r.x1 - r.x0} height={r.y1 - r.y0} fill="none" stroke="transparent" stroke-width={pad * 2} />;
                // Notes preview their first lines on hover (guideline §10.1).
                const tip = x.kind === "note" ? (x.body ?? "").replace(/\s+/g, " ").slice(0, 140) || undefined : undefined;
                return <rect {...common} x={r.x0} y={r.y0} width={r.x1 - r.x0} height={r.y1 - r.y0} fill="transparent" data-tip={tip} aria-label={tip} />;
              })();
            }}
          </For>
        </Show>
        <Show when={pickedHere()}>
          {(a) => {
            const x = (): Annotation => transformed(a(), drag());
            return (
              <g class="picked-frame">
                <rect x={x().rect.x0 - 2} y={x().rect.y0 - 2} width={x().rect.x1 - x().rect.x0 + 4} height={x().rect.y1 - x().rect.y0 + 4} fill="none" />
                <For each={handles(x())}>{(h, i) => <rect class="handle" x={h.x - 3.5 / props.scale * 1.5} y={h.y - 3.5 / props.scale * 1.5} width={7 / props.scale * 1.5} height={7 / props.scale * 1.5} onPointerDown={(e) => startResize(e, a(), i())} />}</For>
              </g>
            );
          }}
        </Show>
      </svg>
      <Show when={pickedHere() && picked()?.edit && !drag()}>
        <Editor paper={props.paper} annotation={pickedHere() as Annotation} scale={props.scale} pageWidth={props.width} pageHeight={props.height} />
      </Show>
      <Show when={pendingHere()}>{(a) => <PendingEditor paper={props.paper} annotation={a()} scale={props.scale} pageWidth={props.width} pageHeight={props.height} />}</Show>
      <Show when={drawing()}>
        <span class="sr-only">{t("annot.drawing_hint")}</span>
      </Show>
    </div>
  );
}
