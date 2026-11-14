// One keymap for the whole UI: actions are named, keys are data (user-remappable later).

export type Action = () => void;

const bindings = new Map<string, Action>();

/** Normalise "Ctrl+Shift+O" / KeyboardEvent into one form. */
export function chord(e: KeyboardEvent | string): string {
  if (typeof e === "string") {
    return e
      .split("+")
      .map((p) => p.trim().toLowerCase())
      .sort((a, b) => order(a) - order(b) || a.localeCompare(b))
      .join("+");
  }
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("ctrl");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  parts.push(e.key.toLowerCase());
  return parts.join("+");
}

function order(p: string): number {
  return ["ctrl", "alt", "shift"].indexOf(p) === -1 ? 9 : ["ctrl", "alt", "shift"].indexOf(p);
}

/** Bind a key chord to an action; returns a function that removes the binding. */
export function bind(keys: string, action: Action): () => void {
  const k = chord(keys);
  bindings.set(k, action);
  return () => bindings.delete(k);
}

const TEXT_KEYS = new Set(["ctrl+z", "ctrl+y", "ctrl+shift+z", "ctrl+a", "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+shift+c"]);

/** Install the global listener once. */
export function installKeymap(target: Window = window): void {
  target.addEventListener("keydown", (e) => {
    const k = chord(e);
    // In a text field, typing and its editing keys (undo, select all, copy, delete…) belong to
    // the field; only other shortcuts with Ctrl or Alt reach the app.
    const el = e.target as HTMLElement | null;
    const editable = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
    if (editable && (!(e.ctrlKey || e.metaKey || e.altKey) || TEXT_KEYS.has(k))) return;
    const action = bindings.get(k);
    if (action) {
      e.preventDefault();
      action();
    }
  });
}
