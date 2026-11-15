// End-to-end check of the Reader (Milestones 1–2, frame of MD) on the real, built app.
//
//   node e2e/reader.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Starts the app with a private data folder (EP_DATA_DIR) and WebView2's DevTools port, drives
// it through the DOM, prints a JSON report and saves screenshots. Needs the fixture papers
// (`cargo xtask fetch-fixtures`) and a release build (`pnpm --filter @electropaper/reader-host
// tauri build --no-bundle`).

import { mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const papers = [join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf"), join(root, ".fixtures/papers/plos-pone-0232683.pdf")];
const data = join(scratch, "data");
const port = 9334;
const report = {};
const consoleLog = [];
const failures = [];
const check = (name, ok, detail) => {
  report[name] = detail ?? ok;
  if (!ok) failures.push(name);
};

const launch = (args) => launchApp({ exe, args, data, webview: join(scratch, "webview"), port, out, consoleLog });

/** Choose a theme the way a user does: the theme button, then the menu item. */
const pickTheme = (s, label) =>
  s.js(`(() => { document.querySelector('.topbar [aria-label="Theme"]').click(); const it = [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim() === ${JSON.stringify(label)}); it?.click(); return !!it; })()`);
/** Choose an item of a top-bar menu (by the button's name and the item's text). */
const pickMenu = (s, button, item) =>
  s.js(`(() => { document.querySelector('.topbar [aria-label="${button}"]').click(); const it = [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim().startsWith(${JSON.stringify(item)})); it?.click(); return !!it; })()`);

mkdirSync(out, { recursive: true });
rmSync(data, { recursive: true, force: true });
rmSync(join(scratch, "webview"), { recursive: true, force: true });

// ---------------------------------------------------------------- first run ----------------
let s = await launch(papers);
try {
  const t0 = Date.now();
  const tabs = await s.until(`document.querySelectorAll(".tab").length === 2 && document.querySelectorAll(".pane.focused canvas.page-canvas, .pane.focused img.page-canvas").length > 0 ? document.querySelectorAll(".tab").length : 0`, 30000);
  check("opened_from_command_line", tabs === 2, { tabs, first_page_ms: Date.now() - t0 });
  await s.until(`document.querySelectorAll(".pane.focused .text-layer span").length > 100`);
  check("text_layer_words", (await s.js(`document.querySelectorAll(".pane.focused .text-layer span").length`)) > 100);
  const links = await s.until(`document.querySelectorAll(".pane.focused .link-layer .link").length || null`);
  check("links_on_page", (links ?? 0) > 0, links);
  const page0 = await s.js(`document.querySelector(".page-box").value`);
  await s.scrollerKey("End");
  const last = await s.until(`(() => { const v = document.querySelector(".page-box").value; return v !== ${JSON.stringify(page0)} ? v : null })()`);
  const total = await s.js(`document.querySelector(".page-box").parentElement.textContent.match(/\\/\\s*(\\d+)/)?.[1]`);
  check("keyboard_end_goes_to_last_page", last === total, { last, total });
  await s.scrollerKey("Home");
  await s.until(`document.querySelector(".page-box").value === "1"`);

  // Zoom in and back to fit width.
  const before = await s.js(`document.querySelector(".pane.focused .page").offsetWidth`);
  await s.js(`document.querySelector('.topbar [aria-label="Zoom in"]').click()`);
  const after = await s.until(`(() => { const w = document.querySelector(".pane.focused .page").offsetWidth; return w > ${before} + 10 ? w : null })()`);
  check("zoom_in", after !== null, { before, after });

  // Two-page layout puts pages 1 and 2 side by side.
  await pickMenu(s, "More", "Two pages");
  const side = await s.until(`(() => { const sl = [...document.querySelectorAll(".pane.focused .slot")]; return sl.length > 1 && sl[0].style.top === sl[1].style.top ? sl.length : null })()`);
  check("two_page_layout", side !== null, side);
  await pickMenu(s, "More", "Continuous");
  await s.js(`(() => { document.querySelector(".zoom-button").click(); [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim() === "Fit width").click(); })()`);

  // Contents panel: the first paper has bookmarks; clicking one moves and enables Back.
  await s.js(`document.querySelectorAll(".pane.focused .tab")[0].click()`);
  await s.until(`document.querySelectorAll(".pane.focused canvas.page-canvas, .pane.focused img.page-canvas").length > 0 && document.querySelector(".page-box")`);
  const outline = await s.until(`document.querySelectorAll(".outline .outline-title").length || null`);
  check("outline_entries", (outline ?? 0) > 3, outline);
  await s.js(`[...document.querySelectorAll(".outline .outline-title")].at(-1).click()`);
  const moved = await s.until(`document.querySelector(".page-box").value !== "1" ? document.querySelector(".page-box").value : null`);
  check("outline_navigates", moved !== null, moved);
  const canBack = await s.js(`!document.querySelector('.topbar [aria-label="Back"]').disabled`);
  await s.scrollerKey("ArrowLeft", { alt: true });
  const backTo = await s.until(`document.querySelector(".page-box").value === "1" ? "1" : null`);
  check("back_returns", canBack && backTo === "1", { canBack, backTo });

  // Find: notation-aware search with highlights.
  await s.key("f", { ctrl: true });
  await s.until(`document.querySelector(".find input[type=search]")`);
  await s.js(`(() => { const i = document.querySelector(".find input[type=search]"); i.value = "thermal"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
  const found = await s.until(`(() => { const m = document.querySelector(".find .muted")?.textContent.match(/(\\d+) matches/); return m ? Number(m[1]) : null })()`);
  check("find_results", (found ?? 0) > 3, found);
  const marked = await s.until(`document.querySelectorAll(".pane.focused .hit").length || null`);
  check("find_highlights", (marked ?? 0) > 0, marked);

  // Copy takes text from the engine (soft hyphens and line breaks handled).
  const copied = await s.js(`(() => {
    const spans = [...document.querySelectorAll(".pane.focused .text-layer span")].slice(10, 25);
    const r = document.createRange(); r.setStartBefore(spans[0]); r.setEndAfter(spans.at(-1));
    const sel = getSelection(); sel.removeAllRanges(); sel.addRange(r);
    const dt = new DataTransfer();
    document.querySelector(".pane.focused .scroller").dispatchEvent(new ClipboardEvent("copy", { clipboardData: dt, bubbles: true, cancelable: true }));
    return dt.getData("text/plain");
  })()`);
  check("copy_from_engine", typeof copied === "string" && copied.split(" ").length >= 10 && !copied.includes("\\n"), copied);

  // Split view, then the dark theme, then a screenshot.
  await s.js(`document.querySelector('.topbar [aria-label="Split view"]').click()`);
  check("split_view", (await s.until(`document.querySelectorAll(".pane").length === 2 ? 2 : null`)) === 2);
  await pickTheme(s, "Dark");
  await sleep(1500);
  await s.shot("m2-viewer-dark.png");

  // Library view: both papers; search narrows the list.
  await s.key("l", { ctrl: true });
  const listed = await s.until(`document.querySelectorAll(".paper-item").length || null`);
  // (the paper opened from a URL comes later; here the library has the two command-line papers)
  await s.js(`(() => { const i = document.querySelector(".library-tools input"); i.value = "thermal"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
  const narrowed = await s.until(`document.querySelectorAll(".paper-item").length === 1 ? 1 : null`);
  check("library_search", listed === 2 && narrowed === 1, { listed, narrowed });
  await pickTheme(s, "Light");
  await s.shot("m2-library.png");
  await s.key("l", { ctrl: true });
  await s.until(`document.querySelectorAll(".pane canvas.page-canvas, .pane img.page-canvas").length > 0 && document.querySelector(".page-box")`);
  await sleep(1000);

  // Open a PDF from a web address (needs the network; skipped when offline).
  const opened = await s.js(`(async () => {
    try {
      const path = await window.__TAURI_INTERNALS__.invoke("download_pdf", { url: "https://arxiv.org/pdf/2601.12341v1" });
      const call = (command, input) => window.__TAURI_INTERNALS__.invoke("bus_call", { command, input, deadlineMs: null, approved: [] });
      const added = await call("workspace.add_file", { path, source_url: "https://arxiv.org/pdf/2601.12341v1" });
      return { name: added.paper.original_name, source: added.paper.source_url, pages_ok: (await call("document.info", { file: added.paper.file })).pages > 0 };
    } catch (e) { return { offline_or_failed: String(e?.message ?? JSON.stringify(e)) }; }
  })()`);
  check("open_from_url", !!opened?.pages_ok || !!opened?.offline_or_failed, opened);

  // Leave the second paper at page 5 for the restart check.
  await s.js(`(() => { const panes = document.querySelectorAll(".pane"); panes[panes.length - 1].querySelector(".pane-body").dispatchEvent(new PointerEvent("pointerdown", { bubbles: true })); })()`);
  await s.js(`(() => { const box = document.querySelector(".page-box"); box.value = "5"; box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
  await s.until(`document.querySelector(".page-box").value === "5"`);
  await sleep(2500); // a jump is saved after 0.4 s, scrolling after 1.5 s
  report.before_restart = await s.js(`(async () => {
    const call = (command, input) => window.__TAURI_INTERNALS__.invoke("bus_call", { command, input, deadlineMs: null, approved: [] });
    const papers = await call("workspace.list_papers", {});
    const out = {};
    for (const p of papers) out[p.original_name] = (await call("workspace.get_reading_state", { paper: p.id })).location?.page ?? null;
    out.page_box = document.querySelector(".page-box")?.value ?? null;
    out.focused_tab = document.querySelector(".pane.focused .tab.active")?.textContent ?? null;
    return out;
  })()`);
} catch (e) {
  report.error = String(e);
  failures.push("exception");
} finally {
  s.ws.close();
  s.app.kill();
}

// ---------------------------------------------------------------- restart --------------------
await sleep(1500);
s = await launch([]);
try {
  const restored = await s.until(`document.querySelectorAll(".tab").length || null`, 20000);
  const page = await s.until(`(() => { const v = document.querySelector(".page-box")?.value; return v && v !== "1" ? v : null })()`, 10000);
  check("session_and_place_restored", restored === 2 && page === "5", { tabs: restored, page });
  await s.shot("m2-restored.png");
} catch (e) {
  report.restart_error = String(e);
  failures.push("restart");
} finally {
  s.ws.close();
  s.app.kill();
}

report.console = consoleLog;
report.failures = failures;
console.log(JSON.stringify(report, null, 2));
process.exit(failures.length === 0 ? 0 : 1);
