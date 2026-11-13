// UI strings. The app is English-only until Phase 3, which adds other languages.
// All visible text still goes through `t()` so that adding a language later is a data change.

/** Every UI string. */
const en = {
  "app.title": "ElectroPaper Reader",
  "library.title": "Library",
  "library.empty": "No papers yet. Add a PDF to start.",
  "library.add": "Add PDF",
  "library.adding": "Adding…",
  "library.pages": "{n} pages",
  "library.untitled": "Untitled",
  "viewer.empty": "Open a paper from the library.",
  "viewer.loading": "Loading…",
  "viewer.page": "Page {n} of {total}",
  "settings.theme": "Theme",
  "theme.light": "Light",
  "theme.dark": "Dark",
  "theme.night": "Night",
  "theme.sepia": "Sepia",
  "error.title": "Something went wrong",
} as const;

export type MessageKey = keyof typeof en;

/** Text for a key, filling `{name}` placeholders. */
export function t(key: MessageKey, vars: Record<string, string | number> = {}): string {
  return en[key].replace(/\{(\w+)\}/g, (_, k: string) => String(vars[k] ?? `{${k}}`));
}
