// Kit components (guideline §8). Every app builds its interface from these; styles in kit.css.

import { Dynamic } from "solid-js/web";
import { For, type JSX, Show, createSignal, onCleanup, onMount, splitProps } from "solid-js";

import { ICONS, type IconName } from "./icons";

// ---------------------------------------------------------------------------------- Icon -----

export interface IconProps {
  name: IconName;
  /** Pixels; defaults to the bar size (20). */
  size?: number;
  class?: string;
}

/** An icon from the one set (decorative: the control around it carries the name). */
export function Icon(props: IconProps): JSX.Element {
  return <Dynamic component={ICONS[props.name]} size={props.size ?? 20} stroke-width={1.75} class={`ep-icon ${props.class ?? ""}`} aria-hidden="true" />;
}

// -------------------------------------------------------------------------------- Buttons ----

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "quiet" | "danger";
  icon?: IconName;
}

/** A text button (verbs; at most one primary per view). */
export function Button(props: ButtonProps): JSX.Element {
  const [local, rest] = splitProps(props, ["variant", "class", "icon", "children"]);
  return (
    <button type="button" {...rest} class={`ep-button ${local.class ?? ""}`} data-variant={local.variant ?? "secondary"}>
      <Show when={local.icon}>{(i) => <Icon name={i()} size={16} />}</Show>
      {local.children}
    </button>
  );
}

export interface IconButtonProps extends Omit<JSX.ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: IconName;
  /** Accessible name and tooltip. */
  label: string;
  /** Shown in the tooltip, e.g. "Ctrl+Z". */
  shortcut?: string;
  /** Toggle state (sets aria-pressed). */
  pressed?: boolean;
  /** A small dot (e.g. unsaved changes). */
  dot?: boolean;
  /** Colour shown as a small bar under the icon (the tool's colour). */
  swatch?: string;
  size?: "sm" | "md";
}

/** An icon-only button: always has a name and a tooltip (guideline §6). */
export function IconButton(props: IconButtonProps): JSX.Element {
  const [local, rest] = splitProps(props, ["icon", "label", "shortcut", "pressed", "dot", "swatch", "size", "class"]);
  return (
    <button
      type="button"
      {...rest}
      class={`ep-icon-button ${local.class ?? ""}`}
      data-size={local.size ?? "md"}
      aria-label={local.label}
      aria-pressed={local.pressed === undefined ? undefined : local.pressed}
      data-tip={local.shortcut ? `${local.label} (${local.shortcut})` : local.label}
    >
      <Icon name={local.icon} size={local.size === "sm" ? 16 : 20} />
      <Show when={local.dot}>
        <span class="ep-dot" aria-hidden="true" />
      </Show>
      <Show when={local.swatch}>
        <span class="ep-swatch-bar" style={{ background: local.swatch }} aria-hidden="true" />
      </Show>
    </button>
  );
}

// ------------------------------------------------------------------------------ Choices ------

export interface SegmentedProps<T extends string> {
  label: string;
  value: T;
  options: { value: T; label: string; icon?: IconName }[];
  onChange: (v: T) => void;
}

/** Two to four exclusive choices. */
export function Segmented<T extends string>(props: SegmentedProps<T>): JSX.Element {
  return (
    <div class="ep-segmented" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <button type="button" role="radio" aria-checked={props.value === o.value} aria-label={o.label} data-tip={o.icon ? o.label : undefined} onClick={() => props.onChange(o.value)}>
            {o.icon ? <Icon name={o.icon} size={16} /> : o.label}
          </button>
        )}
      </For>
    </div>
  );
}

/** A row of colour choices from a fixed palette (guideline §3.3). */
export function Swatches(props: { label: string; colors: readonly string[]; value: string | null; onPick: (c: string) => void; names?: Record<string, string> }): JSX.Element {
  return (
    <div class="ep-swatches" role="radiogroup" aria-label={props.label}>
      <For each={props.colors}>
        {(c) => (
          <button
            type="button"
            class="ep-swatch"
            role="radio"
            aria-checked={props.value === c}
            aria-label={props.names?.[c] ?? c}
            data-tip={props.names?.[c]}
            style={{ background: c }}
            onClick={() => props.onPick(c)}
          />
        )}
      </For>
    </div>
  );
}

/** A keyboard shortcut. */
export function Kbd(props: { children: string }): JSX.Element {
  return <kbd class="ep-kbd">{props.children}</kbd>;
}

// --------------------------------------------------------------------------- Containers -----

export interface CardProps {
  title: string;
  icon?: IconName;
  meta?: string;
  onClose?: () => void;
  closeLabel?: string;
  children: JSX.Element;
  footer?: JSX.Element;
  class?: string;
  style?: JSX.CSSProperties;
  ref?: (el: HTMLDivElement) => void;
}

/** A thing on the page or in a panel: header, body, footer (guideline §8 Card). */
export function Card(props: CardProps): JSX.Element {
  return (
    <div class={`ep-card ${props.class ?? ""}`} style={props.style} ref={props.ref} role="group" aria-label={props.title}>
      <header class="ep-card-head">
        <Show when={props.icon}>{(i) => <Icon name={i()} size={16} />}</Show>
        <strong>{props.title}</strong>
        <span class="ep-card-meta">{props.meta ?? ""}</span>
        <Show when={props.onClose}>
          <IconButton icon="x" size="sm" label={props.closeLabel ?? "Close"} shortcut="Esc" onClick={() => props.onClose?.()} />
        </Show>
      </header>
      <div class="ep-card-body">{props.children}</div>
      <Show when={props.footer}>
        <footer class="ep-card-foot">{props.footer}</footer>
      </Show>
    </div>
  );
}

export interface PanelTab {
  id: string;
  icon: IconName;
  label: string;
  shortcut?: string;
}

/** A side panel with icon tabs at its top. */
export function Panel(props: { label: string; tabs: PanelTab[]; active: string; onTab: (id: string) => void; onClose: () => void; closeLabel: string; children: JSX.Element; side: "left" | "right" }): JSX.Element {
  return (
    <aside class="ep-panel" data-side={props.side} aria-label={props.label}>
      <div class="ep-panel-tabs" role="tablist">
        <For each={props.tabs}>
          {(tb) => (
            <IconButton icon={tb.icon} label={tb.label} shortcut={tb.shortcut} role="tab" aria-selected={props.active === tb.id} pressed={props.active === tb.id} onClick={() => props.onTab(tb.id)} />
          )}
        </For>
        <span class="ep-spacer" />
        <IconButton icon="x" size="sm" label={props.closeLabel} onClick={props.onClose} />
      </div>
      <div class="ep-panel-title">{props.tabs.find((tb) => tb.id === props.active)?.label ?? ""}</div>
      <div class="ep-panel-body">{props.children}</div>
    </aside>
  );
}

/** What a panel or list is for, when it has nothing yet, with one action. */
export function EmptyState(props: { icon: IconName; text: string; action?: JSX.Element }): JSX.Element {
  return (
    <div class="ep-empty">
      <Icon name={props.icon} size={28} />
      <p>{props.text}</p>
      <Show when={props.action}>{props.action}</Show>
    </div>
  );
}

/** Neutral placeholder lines while a panel's content loads. */
export function Skeleton(props: { lines?: number }): JSX.Element {
  return (
    <div class="ep-skeleton" aria-busy="true">
      <For each={Array.from({ length: props.lines ?? 4 })}>{(_, i) => <span style={{ width: `${90 - ((i() * 17) % 40)}%` }} />}</For>
    </div>
  );
}

/** A problem that blocks work: one sentence and one action. */
export function Banner(props: { children: JSX.Element; onClose?: () => void; closeLabel?: string; tone?: "error" | "info" }): JSX.Element {
  return (
    <div class="ep-banner" data-tone={props.tone ?? "error"} role="alert">
      <Icon name="circle-alert" size={16} />
      <div class="ep-banner-text">{props.children}</div>
      <Show when={props.onClose}>
        <IconButton icon="x" size="sm" label={props.closeLabel ?? "Close"} onClick={() => props.onClose?.()} />
      </Show>
    </div>
  );
}

/** The status line (24 px): items on the left, items on the right. */
export function StatusLine(props: { left: JSX.Element; right: JSX.Element }): JSX.Element {
  return (
    <footer class="ep-status" role="status" aria-live="polite">
      <div class="ep-status-side">{props.left}</div>
      <div class="ep-status-side">{props.right}</div>
    </footer>
  );
}

/** One status item: optional icon, short text, optional click for details. */
export function StatusItem(props: { icon?: IconName; tone?: "muted" | "accent" | "danger" | "success"; children: JSX.Element; onClick?: () => void; label?: string }): JSX.Element {
  const content = (
    <>
      <Show when={props.icon}>{(i) => <Icon name={i()} size={14} />}</Show>
      <span>{props.children}</span>
    </>
  );
  return (
    <Show when={props.onClick} fallback={<span class="ep-status-item" data-tone={props.tone ?? "muted"}>{content}</span>}>
      <button type="button" class="ep-status-item" data-tone={props.tone ?? "muted"} onClick={() => props.onClick?.()} aria-label={props.label}>
        {content}
      </button>
    </Show>
  );
}

// ------------------------------------------------------------------------------ Dialog ------

/** Stops the app to ask: title, one paragraph, at most two buttons. Esc cancels. */
/** A dialog. `size="lg"` is for settings and lists; content never sets its own width (§5.4, §9.9). */
export function Dialog(props: { title: string; children: JSX.Element; actions: JSX.Element; onCancel: () => void; size?: "lg" }): JSX.Element {
  let box!: HTMLDivElement;
  onMount(() => {
    const prev = document.activeElement as HTMLElement | null;
    (box.querySelector("[data-autofocus], input, textarea, button[data-variant='primary'], button") as HTMLElement | null)?.focus();
    onCleanup(() => prev?.focus?.());
  });
  return (
    <div class="ep-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onCancel()}>
      <div
        class="ep-dialog"
        data-size={props.size}
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        ref={box}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            props.onCancel();
          }
        }}
      >
        <h2>{props.title}</h2>
        <div class="ep-dialog-body">{props.children}</div>
        <div class="ep-dialog-actions">{props.actions}</div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------------------- Menus ------

export type MenuItem =
  | { label: string; icon?: IconName; shortcut?: string; run: () => void; danger?: boolean; checked?: boolean; disabled?: boolean }
  | { separator: true }
  | { heading: string };

const [menu, setMenu] = createSignal<{ items: MenuItem[]; x: number; y: number; label: string } | null>(null);

/** Open a menu at a point or under an element. */
export function openMenu(label: string, items: MenuItem[], at: { x: number; y: number } | DOMRect): void {
  const p = "width" in at ? { x: at.left, y: at.bottom + 4 } : at;
  setMenu({ items, x: p.x, y: p.y, label });
}

export function closeMenu(): void {
  setMenu(null);
}

/** Renders the open menu (one per window; mount once in the app). */
export function MenuHost(): JSX.Element {
  let box: HTMLDivElement | undefined;
  const onDown = (e: PointerEvent): void => {
    if (menu() && box && !box.contains(e.target as Node)) closeMenu();
  };
  const onKey = (e: KeyboardEvent): void => {
    if (!menu() || !box) return;
    const items = [...box.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(i + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(i - 1 + items.length) % items.length]?.focus();
    }
  };
  onMount(() => {
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    onCleanup(() => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    });
  });
  return (
    <Show when={menu()}>
      {(m) => {
        const left = Math.min(m().x, window.innerWidth - 260);
        const top = Math.min(m().y, window.innerHeight - 40 - m().items.length * 30);
        onMount(() => box?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus());
        return (
          <div class="ep-menu" role="menu" aria-label={m().label} ref={box} style={{ left: `${Math.max(4, left)}px`, top: `${Math.max(4, top)}px` }}>
            <For each={m().items}>
              {(it) => {
                if ("separator" in it) return <div class="ep-menu-sep" role="separator" />;
                if ("heading" in it) return <div class="ep-menu-heading">{it.heading}</div>;
                return (
                  <button
                    type="button"
                    role={it.checked === undefined ? "menuitem" : "menuitemradio"}
                    aria-checked={it.checked}
                    class="ep-menu-item"
                    data-danger={it.danger ? "" : undefined}
                    disabled={it.disabled}
                    onClick={() => {
                      closeMenu();
                      it.run();
                    }}
                  >
                    <span class="ep-menu-icon">{it.checked ? <Icon name="check" size={16} /> : it.icon ? <Icon name={it.icon} size={16} /> : null}</span>
                    <span class="ep-menu-label">{it.label}</span>
                    <Show when={it.shortcut}>
                      <span class="ep-menu-key">{it.shortcut}</span>
                    </Show>
                  </button>
                );
              }}
            </For>
          </div>
        );
      }}
    </Show>
  );
}

// ------------------------------------------------------------------------------- Toasts -----

const [toast, setToast] = createSignal<{ text: string; action?: { label: string; run: () => void }; id: number } | null>(null);
let toastTimer: number | undefined;

/** Confirm an action whose result is not visible on screen (one at a time, 5 s). */
export function showToast(text: string, action?: { label: string; run: () => void }): void {
  window.clearTimeout(toastTimer);
  const id = Date.now();
  setToast({ text, action, id });
  toastTimer = window.setTimeout(() => setToast((t) => (t?.id === id ? null : t)), 5000);
}

export function ToastHost(): JSX.Element {
  return (
    <Show when={toast()}>
      {(t) => (
        <div class="ep-toast" role="status">
          <span>{t().text}</span>
          <Show when={t().action}>
            {(a) => (
              <button
                type="button"
                class="ep-toast-action"
                onClick={() => {
                  setToast(null);
                  a().run();
                }}
              >
                {a().label}
              </button>
            )}
          </Show>
        </div>
      )}
    </Show>
  );
}
