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

/** Install the global listener once. */
export function installKeymap(target: Window = window): void {
  target.addEventListener("keydown", (e) => {
    const action = bindings.get(chord(e));
    if (action) {
      e.preventDefault();
      action();
    }
  });
}
