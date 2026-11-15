// The popover that shows a figure, table, equation, region or reference in place (P1.M4.LNK.3,
// OBJ.5, EQ.3, TAB.2). One at a time; Esc or a click elsewhere closes it; "Go to" moves the view
// and Back returns to the sentence.

import type { BlobRef, DocObject, Reference, RegionHit } from "@electropaper/bindings";
import { Button, Card, IconButton, type IconName, Skeleton, showToast, t } from "@electropaper/kit";
import { For, type JSX, Show, createResource, onCleanup, onMount } from "solid-js";

import { bus, openExternal, renderUrl } from "../host";
import { quoteToNotes } from "../notes/state";
import { hoverLeave, hoverStay, type Peek, peek, setPeek, setSettingsOpen } from "./state";
import { TableView } from "./TableView";
import { createSignal } from "solid-js";

const W = 420;

const KIND: Record<string, { icon: IconName; label: () => string }> = {
  figure: { icon: "image", label: () => t("intel.figure") },
  table: { icon: "table", label: () => t("intel.table") },
  equation: { icon: "sigma", label: () => t("intel.equation") },
  heading: { icon: "list-tree", label: () => t("intel.section") },
  caption: { icon: "image", label: () => t("intel.figure") },
};

function place(at: DOMRect, h = 360): JSX.CSSProperties {
  const left = Math.max(8, Math.min(window.innerWidth - W - 8, at.left + at.width / 2 - W / 2));
  const below = at.bottom + 8;
  const top = below + h > window.innerHeight - 30 ? Math.max(8, at.top - h - 8) : below;
  return { left: `${left}px`, top: `${top}px`, width: `${W}px` };
}

async function copyImage(hash: string): Promise<void> {
  const blob = await (await fetch(renderUrl(hash))).blob();
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
  showToast(t("intel.copied_image"));
}

const refsCache = new Map<string, Promise<Reference[]>>();
function referencesOf(file: BlobRef): Promise<Reference[]> {
  let p = refsCache.get(file.hash);
  if (!p) {
    p = bus("document.references", { file }).catch(() => []);
    refsCache.set(file.hash, p);
  }
  return p;
}

export function PeekHost(props: { paper: string; goTo: (page: number, y: number) => void }): JSX.Element {
  let box: HTMLDivElement | undefined;
  onMount(() => {
    const down = (e: PointerEvent): void => {
      if (peek() && box && !box.contains(e.target as Node) && !(e.target as HTMLElement).closest(".mention")) setPeek(null);
    };
    const key = (e: KeyboardEvent): void => {
      if (e.key === "Escape" && peek()) {
        e.stopPropagation();
        setPeek(null);
      }
    };
    // Scrolling or zooming the page closes a hover popover (design language §17.7).
    const moved = (e: Event): void => {
      const p = peek();
      if (!p?.hover) return;
      if (e.type === "wheel" || (e.target instanceof HTMLElement && e.target.classList.contains("scroller"))) setPeek(null);
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("scroll", moved, true);
    window.addEventListener("wheel", moved, { capture: true, passive: true });
    onCleanup(() => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("scroll", moved, true);
      window.removeEventListener("wheel", moved, true);
    });
  });
  return (
    <Show when={peek()} keyed>
      {(p) => (
        <div class="peek" ref={box} style={place(p.at)} onPointerEnter={hoverStay} onPointerLeave={() => p.hover && hoverLeave()}>
          {p.kind === "citation" ? (
            <CitationPeek p={p} goTo={props.goTo} />
          ) : p.kind === "loading" ? (
            <Card title={p.title} icon="image" onClose={() => setPeek(null)} closeLabel={t("common.close")}>
              <Skeleton lines={4} />
            </Card>
          ) : (
            <RegionPeek p={p} paper={props.paper} goTo={props.goTo} />
          )}
        </div>
      )}
    </Show>
  );
}

function CitationPeek(props: { p: Extract<Peek, { kind: "citation" }>; goTo: (page: number, y: number) => void }): JSX.Element {
  const [refs] = createResource(() => referencesOf(props.p.file));
  const entries = (): Reference[] => (refs() ?? []).filter((r) => props.p.marker.entries.includes(r.index));
  return (
    <Card title={props.p.marker.text} icon="quote" onClose={() => setPeek(null)} closeLabel={t("common.close")}>
      <Show when={refs()} fallback={<Skeleton lines={3} />}>
        <ul class="peek-refs">
          <For each={entries()}>
            {(r) => (
              <li>
                <p>{r.text}</p>
                <div class="row">
                  <Button variant="quiet" icon="arrow-right" onClick={() => (setPeek(null), props.goTo(r.page, Math.max(0, r.rect.y0 - 40)))}>
                    {t("intel.go_to")}
                  </Button>
                  <Show when={r.doi}>{(d) => <Button variant="quiet" icon="external-link" onClick={() => void openExternal(`https://doi.org/${d()}`)}>DOI</Button>}</Show>
                  <Show when={r.arxiv}>{(a) => <Button variant="quiet" icon="external-link" onClick={() => void openExternal(`https://arxiv.org/abs/${a()}`)}>arXiv</Button>}</Show>
                  <Show when={!r.doi && !r.arxiv && r.url}>{(u) => <Button variant="quiet" icon="external-link" onClick={() => void openExternal(u())}>{t("intel.open_link")}</Button>}</Show>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </Card>
  );
}

function RegionPeek(props: { p: Exclude<Peek, { kind: "citation" } | { kind: "loading" }>; paper: string; goTo: (page: number, y: number) => void }): JSX.Element {
  const [table, setTable] = createSignal(false);
  const request = (): { page: number; rect?: unknown; point?: unknown } => {
    const p = props.p;
    if (p.kind === "object") return { page: p.object.rect.page, rect: p.object.rect };
    const r = p.rect;
    return r.x1 - r.x0 < 1 && r.y1 - r.y0 < 1 ? { page: p.page, point: { x: r.x0, y: r.y0 } } : { page: p.page, rect: { page: p.page, ...r } };
  };
  const [hit] = createResource(async (): Promise<RegionHit | { error: string }> => {
    try {
      const q = request();
      return await bus("document.region", { file: props.p.file, page: q.page, ...(q.rect ? { rect: q.rect as RegionHit["rect"] } : {}), ...(q.point ? { point: q.point as { x: number; y: number } } : {}), width_px: 840 });
    } catch (e) {
      return { error: String((e as Error).message ?? e) };
    }
  });
  const object = (): DocObject | null | undefined => {
    const h = hit();
    return h && "rect" in h ? h.object : props.p.kind === "object" ? props.p.object : null;
  };
  const kind = (): { icon: IconName; label: () => string } => KIND[object()?.kind ?? ""] ?? { icon: "square", label: () => t("intel.region") };
  const [eq] = createResource(
    () => (object()?.kind === "equation" ? object() : null),
    (o) => bus("document.equation", { file: props.p.file, object: { page: o.rect.page, id: o.id } }).catch(() => null),
  );
  // "Figure 3" and "Table 2" are printed labels; sections and equations need their word.
  const title = (): string => {
    const o = object();
    if (!o?.label) return kind().label();
    if (o.kind === "heading") return `${t("intel.section")} ${o.label}`;
    if (o.kind === "equation") return `${t("intel.equation")} ${o.label}`;
    return o.label;
  };
  return (
    <Card
      title={title()}
      icon={kind().icon}
      meta={object() ? `${t("viewer.page_box")} ${object()?.rect.page}` : ""}
      onClose={() => setPeek(null)}
      closeLabel={t("common.close")}
      footer={
        <Show when={hit() && "rect" in (hit() as object) ? (hit() as RegionHit) : null}>
          {(h) => (
            <>
              <Show when={props.p.kind === "object"}>
                <Button variant="quiet" icon="arrow-right" onClick={() => (setPeek(null), props.goTo(h().rect.page, Math.max(0, h().rect.y0 - 40)))}>
                  {t("intel.go_to")}
                </Button>
              </Show>
              <span class="ep-spacer" />
              <Show when={object()?.kind === "table"}>
                <IconButton icon="table" size="sm" label={t("intel.open_table")} onClick={() => setTable(true)} />
              </Show>
              <Show when={object()?.kind === "equation"}>
                <IconButton
                  icon="sigma"
                  size="sm"
                  label={eq()?.latex ? t("intel.copy_latex") : (eq()?.note ?? t("intel.copy_latex"))}
                  disabled={!eq()?.latex}
                  onClick={() => void navigator.clipboard.writeText(eq()?.latex ?? "").then(() => showToast(t("intel.copied_latex")))}
                />
              </Show>
              <IconButton icon="copy" size="sm" label={t("intel.copy_text")} disabled={!h().text} onClick={() => void navigator.clipboard.writeText(h().text).then(() => showToast(t("intel.copied_text")))} />
              <IconButton icon="image" size="sm" label={t("intel.copy_image")} onClick={() => void copyImage(h().image.hash).catch(() => showToast(t("intel.copy_failed")))} />
              <IconButton
                icon="notebook-pen"
                size="sm"
                label={t("notes.add_to_notes")}
                onClick={() => {
                  setPeek(null);
                  void quoteToNotes(props.paper, { text: object()?.text ?? h().text, page: h().rect.page, y: h().rect.y0 });
                }}
              />
            </>
          )}
        </Show>
      }
    >
      <Show when={hit()} fallback={<Skeleton lines={5} />}>
        {(h) => (
          <Show when={"rect" in h() ? (h() as RegionHit) : null} fallback={<p class="muted">{t("intel.nothing_here")}</p>}>
            {(r) => (
              <>
                <img class="peek-image" src={renderUrl(r().image.hash)} alt={title()} />
                <Show when={object()?.kind === "equation"}>
                  <Show
                    when={!eq.loading}
                    fallback={<p class="muted small">{t("intel.reading_formula")}</p>}
                  >
                    <Show
                      when={eq()?.latex}
                      fallback={
                        <button type="button" class="link-button small" onClick={() => (setPeek(null), setSettingsOpen(true))}>
                          {t("intel.latex_offer")}
                        </button>
                      }
                    >
                      {(l) => (
                        <pre class="latex" aria-label={t("intel.latex")}>
                          {l()}
                        </pre>
                      )}
                    </Show>
                  </Show>
                </Show>
                <Show when={object()?.text && object()?.kind !== "equation"}>
                  <p class="peek-caption">{object()?.text}</p>
                </Show>
              </>
            )}
          </Show>
        )}
      </Show>
      <Show when={table() && object()}>{(o) => <TableView file={props.p.file} object={o()} onClose={() => setTable(false)} />}</Show>
    </Card>
  );
}
