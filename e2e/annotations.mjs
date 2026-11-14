// End-to-end check of annotations (Milestone 3) on the real, built app.
//
//   node e2e/annotations.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Marks up a paper the way a user does (selection bar, tools, the card, the Notes panel, undo,
// delete, move), saves into the PDF, reads the saved PDF with PDF.js as an independent parser,
// and restarts the app to check that everything is still there.

import { readdirSync, readFileSync, mkdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const paper = join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf");
const data = join(scratch, "annot-data");
const webview = join(scratch, "annot-webview");
const port = 9335;
const report = {};
const consoleLog = [];
const failures = [];
const check = (name, ok, detail) => {
  report[name] = detail ?? ok;
  if (!ok) failures.push(name);
};
const launch = (args) => launchApp({ exe, args, data, webview, port, out, consoleLog });

mkdirSync(out, { recursive: true });
rmSync(data, { recursive: true, force: true });
rmSync(webview, { recursive: true, force: true });

// Page helpers, evaluated in the app.
const HELPERS = `
window.__t = {
  page: (n = 1) => document.querySelector('.pane.focused .page[data-page="' + n + '"]'),
  select(from, to, n = 1) {
    const spans = [...this.page(n).querySelectorAll(".text-layer span")].slice(from, to);
    const r = document.createRange(); r.setStartBefore(spans[0]); r.setEndAfter(spans.at(-1));
    const s = getSelection(); s.removeAllRanges(); s.addRange(r);
    document.querySelector(".pane.focused .scroller").dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  },
  /** Drag on page n from (x0, y0) to (x1, y1), in fractions of the page size. */
  drag(x0, y0, x1, y1, n = 1, steps = 8) {
    const host = this.page(n).querySelector(".annot-host");
    const b = host.getBoundingClientRect();
    const at = (fx, fy) => ({ clientX: b.left + fx * b.width, clientY: b.top + fy * b.height, bubbles: true, button: 0, pointerId: 1 });
    host.dispatchEvent(new PointerEvent("pointerdown", at(x0, y0)));
    for (let i = 1; i <= steps; i++) {
      const f = i / steps;
      window.dispatchEvent(new PointerEvent("pointermove", at(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f + Math.sin(f * 6) * 0.01)));
    }
    window.dispatchEvent(new PointerEvent("pointerup", at(x1, y1)));
  },
  button(text, scope = document) { return [...scope.querySelectorAll("button")].find((b) => b.textContent.trim() === text); },
  type(el, text) { el.value = text; el.dispatchEvent(new InputEvent("input", { bubbles: true })); },
  async list() {
    const tab = await window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] });
    const p = tab[0];
    return window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.annotations", input: { paper: p.id, file: p.file }, deadlineMs: null, approved: [] });
  },
};
true`;

const count = (s) => s.js(`window.__t.list().then((x) => x.annotations.length)`);

// ---------------------------------------------------------------- first run ----------------
let s = await launch([paper]);
let paperBefore;
try {
  await s.until(`document.querySelectorAll(".pane.focused .page[data-page='1'] .text-layer span").length > 100`, 30000);
  await s.js(HELPERS);
  check("tool_bar", !!(await s.until(`document.querySelector(".annot-bar") ? true : null`)));
  paperBefore = await s.js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] }).then((p) => p[0].file.hash)`);

  // 1. Select text → the bar → a green highlight, stored at once (Scholar parity, UI.9).
  await s.js(`window.__t.select(30, 36)`);
  const bar = await s.until(`document.querySelector(".selection-bar") ? true : null`);
  await s.js(`document.querySelectorAll(".selection-bar .swatch")[1].click()`);
  const hl = await s.until(`document.querySelectorAll(".pane.focused .annot-layer polygon.hl").length || null`);
  const first = await s.js(`window.__t.list().then((x) => x.annotations[0])`);
  check("highlight_from_selection", !!bar && hl > 0 && first?.kind === "highlight" && first.color === "#7bd88f" && (first.quads ?? []).length > 0 && (first.text ?? "").length > 3, {
    bar: !!bar,
    polygons: hl,
    text: first?.text,
    quads: first?.quads?.length,
  });
  check("saved_indicator", (await s.until(`document.querySelector(".annot-bar .saved")?.textContent === "Saved" ? true : null`)) === true);

  // 2. Comment on a selection: the card, then Save.
  await s.js(`window.__t.select(60, 64)`);
  await s.until(`document.querySelector(".selection-bar")`);
  await s.js(`window.__t.button("Comment", document.querySelector(".selection-bar")).click()`);
  await s.until(`document.querySelector(".annot-card.pending textarea")`);
  await s.js(`(() => { window.__t.type(document.querySelector(".annot-card.pending textarea"), "Why this assumption?"); window.__t.button("Save", document.querySelector(".annot-card.pending")).click(); })()`);
  const commented = await s.until(`window.__t.list().then((x) => x.annotations.find((a) => a.body === "Why this assumption?")?.kind ?? null)`);
  check("comment_on_selection", commented === "highlight", commented);

  // 3. Tools: note (N), ink (D), rectangle, arrow, free text (T).
  await s.key("n");
  await s.until(`document.querySelector(".annot-host.drawing")`);
  await s.js(`window.__t.drag(0.85, 0.2, 0.85, 0.2)`);
  await s.until(`document.querySelector(".annot-card.pending textarea")`);
  await s.js(`(() => { window.__t.type(document.querySelector(".annot-card.pending textarea"), "A note on the margin"); window.__t.button("Save", document.querySelector(".annot-card.pending")).click(); })()`);
  await s.key("d");
  await s.js(`window.__t.drag(0.1, 0.92, 0.4, 0.95, 1, 40)`);
  await s.js(`window.__t.button("Rectangle", document.querySelector(".annot-bar")).click()`);
  await s.js(`window.__t.drag(0.1, 0.05, 0.9, 0.12)`);
  await s.js(`window.__t.button("Arrow", document.querySelector(".annot-bar")).click()`);
  await s.js(`window.__t.drag(0.05, 0.4, 0.2, 0.35)`);
  await s.key("t");
  await s.js(`window.__t.drag(0.55, 0.85, 0.9, 0.9)`);
  await s.until(`document.querySelector(".annot-card.pending textarea")`);
  await s.js(`(() => { window.__t.type(document.querySelector(".annot-card.pending textarea"), "Check this on page 3"); window.__t.button("Save", document.querySelector(".annot-card.pending")).click(); })()`);
  await sleep(500);
  const kinds = await s.js(`window.__t.list().then((x) => x.annotations.map((a) => a.kind).sort())`);
  check("all_tools", ["arrow", "free_text", "highlight", "ink", "note", "rectangle"].every((k) => kinds?.includes(k)), kinds);
  const ink = await s.js(`window.__t.list().then((x) => x.annotations.find((a) => a.kind === "ink"))`);
  check("ink_simplified", ink && ink.paths[0].length >= 3 && ink.paths[0].length < 41, ink?.paths?.[0]?.length);
  await s.key("Escape");
  await s.key("v");
  await s.shot("m3-annotations.png");

  // 4. Undo and redo (Ctrl+Z / Ctrl+Y).
  const n0 = await count(s);
  await s.key("z", { ctrl: true });
  const n1 = await s.until(`window.__t.list().then((x) => x.annotations.length === ${n0 - 1} ? x.annotations.length : null)`);
  await s.key("y", { ctrl: true });
  const n2 = await s.until(`window.__t.list().then((x) => x.annotations.length === ${n0} ? x.annotations.length : null)`);
  check("undo_redo", n1 === n0 - 1 && n2 === n0, { n0, n1, n2 });

  // 5. The Notes panel: search, click → the card; reply and resolve there.
  await s.js(`window.__t.button("Notes", document.querySelector(".panel-tabs")).click()`);
  const rows = await s.until(`document.querySelectorAll(".notes-list .note-row").length || null`);
  await s.js(`window.__t.type(document.querySelector(".notes-panel input[type=search]"), "assumption")`);
  const filtered = await s.until(`document.querySelectorAll(".notes-list .note-row").length === 1 ? 1 : null`);
  await s.js(`document.querySelector(".notes-list .note-row").click()`);
  const card = await s.until(`document.querySelector(".annot-card:not(.pending)") ? true : null`);
  await s.js(`(() => { const c = document.querySelector(".annot-card"); const i = c.querySelector(".row input.ep-input"); window.__t.type(i, "Because of the boundary layer."); i.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
  const replied = await s.until(`document.querySelectorAll(".annot-card .replies li").length || null`);
  await s.js(`document.querySelector(".annot-card footer input[type=checkbox]").click()`);
  const resolved = await s.until(`window.__t.list().then((x) => x.annotations.find((a) => a.body === "Why this assumption?")?.resolved || null)`);
  check("notes_panel", rows >= 6 && filtered === 1 && card && replied === 1 && resolved === true, { rows, filtered, card, replied, resolved });
  await s.js(`window.__t.type(document.querySelector(".notes-panel input[type=search]"), "")`);
  await s.key("Escape");
  await s.js(`document.querySelector(".pane.focused .scroller").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);

  // 6. Move the rectangle; delete it with the Delete key; undo brings it back.
  const rect0 = await s.js(`window.__t.list().then((x) => x.annotations.find((a) => a.kind === "rectangle").rect)`);
  await s.js(`(() => {
    const proxy = [...window.__t.page(1).querySelectorAll(".annot-proxy")].find((p) => p.tagName === "rect" && p.getAttribute("fill") === "none");
    const b = proxy.getBoundingClientRect();
    const at = (dx, dy) => ({ clientX: b.left + 1 + dx, clientY: b.top + b.height / 2 + dy, bubbles: true, button: 0, pointerId: 1 });
    proxy.dispatchEvent(new PointerEvent("pointerdown", at(0, 0)));
    for (let i = 1; i <= 5; i++) window.dispatchEvent(new PointerEvent("pointermove", at(i * 8, i * 6)));
    window.dispatchEvent(new PointerEvent("pointerup", at(40, 30)));
  })()`);
  const rect1 = await s.until(`window.__t.list().then((x) => { const r = x.annotations.find((a) => a.kind === "rectangle").rect; return r.x0 !== ${rect0.x0} ? r : null; })`);
  check("move", !!rect1 && rect1.x0 > rect0.x0 && rect1.y0 > rect0.y0, { before: rect0, after: rect1 });
  const nBefore = await count(s);
  await s.key("Delete");
  const nDel = await s.until(`window.__t.list().then((x) => x.annotations.length === ${nBefore - 1} ? x.annotations.length : null)`);
  await s.key("z", { ctrl: true });
  const nBack = await s.until(`window.__t.list().then((x) => x.annotations.length === ${nBefore} ? x.annotations.length : null)`);
  check("delete_and_undo", nDel === nBefore - 1 && nBack === nBefore, { nBefore, nDel, nBack });

  // 7. Save into PDF (Ctrl+S): a new managed copy, nothing duplicated, the indicator says so.
  const nSave = await count(s);
  await s.key("s", { ctrl: true });
  const inPdf = await s.until(`document.querySelector(".annot-bar .ep-button")?.textContent.includes("Saved in PDF") ? true : null`, 20000);
  const after = await s.js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] }).then((p) => p[0].file.hash)`);
  const nAfter = await count(s);
  check("save_into_pdf", inPdf && after !== paperBefore && nAfter === nSave, { inPdf, before: paperBefore, after, nSave, nAfter });
  report.saved_hash = after;

  // Dark theme with annotations.
  await s.js(`(() => { const sel = [...document.querySelectorAll(".topbar select")].at(-1); sel.value = "dark"; sel.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await sleep(1500);
  await s.shot("m3-annotations-dark.png");
  await s.js(`(() => { const sel = [...document.querySelectorAll(".topbar select")].at(-1); sel.value = "light"; sel.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await sleep(500);
} catch (e) {
  report.error = String(e?.stack ?? e);
  failures.push("exception");
} finally {
  s.ws.close();
  s.app.kill();
}

// ------------------------------------------------- the saved PDF, read by PDF.js -------------------
try {
  const find = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) {
        const r = find(p);
        if (r) return r;
      } else if (e.name === report.saved_hash) return p;
    }
    return null;
  };
  const file = report.saved_hash ? find(data) : null;
  const original = readFileSync(paper);
  const saved = file ? readFileSync(file) : Buffer.alloc(0);
  check("incremental_prefix", saved.length > original.length && saved.subarray(0, original.length).equals(original), { original: original.length, saved: saved.length });
  const require = createRequire(join(root, "ui/reader/package.json"));
  const pdfjs = await import(pathToFileURL(require.resolve("pdfjs-dist/legacy/build/pdf.mjs")).href);
  const doc = await pdfjs.getDocument({ data: new Uint8Array(saved), isEvalSupported: false, verbosity: 0 }).promise;
  const page = await doc.getPage(1);
  const anns = (await page.getAnnotations()).filter((a) => a.subtype !== "Link");
  const subtypes = [...new Set(anns.map((a) => a.subtype))].sort();
  const reply = anns.find((a) => a.inReplyTo && a.contentsObj?.str === "Because of the boundary layer.");
  check("pdfjs_reads_saved_pdf", ["FreeText", "Highlight", "Ink", "Line", "Square", "Text"].every((k) => subtypes.includes(k)) && !!reply, { subtypes, reply_linked: !!reply });
} catch (e) {
  report.pdfjs_error = String(e?.stack ?? e);
  failures.push("pdfjs");
}

// ---------------------------------------------------------------- restart --------------------
await sleep(1500);
s = await launch([]);
try {
  await s.until(`document.querySelectorAll(".pane.focused .page[data-page='1'] .text-layer span").length > 100`, 30000);
  await s.js(HELPERS);
  const drawn = await s.until(`document.querySelectorAll(".pane.focused .page[data-page='1'] .annot-layer g[data-annotation]").length || null`);
  const st = await s.js(`window.__t.list()`);
  check("annotations_after_restart", drawn >= 7 && st.annotations.length >= 8 && st.can_undo === true && st.unsaved === false, { drawn, stored: st.annotations.length, can_undo: st.can_undo, unsaved: st.unsaved });
  await s.shot("m3-restored.png");
} catch (e) {
  report.restart_error = String(e?.stack ?? e);
  failures.push("restart");
} finally {
  s.ws.close();
  s.app.kill();
}

report.console = consoleLog;
report.failures = failures;
console.log(JSON.stringify(report, null, 2));
process.exit(failures.length === 0 ? 0 : 1);
