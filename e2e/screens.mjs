// Design review screenshots (P1.MD.QA.1, guideline §15): the main screens in all five themes,
// plus the kit gallery, and the accessibility checks of guideline §13 that a DOM can answer:
// every icon-only control has a name and a tooltip, and the top bar fits on one row.
//
//   node e2e/screens.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const paper = join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf");
const data = join(scratch, "screens-data");
const webview = join(scratch, "screens-webview");
const themes = { light: "Light", sepia: "Sepia", dark: "Dark", night: "Night", contrast: "High contrast" };
/** Choose a theme the way a user does: the theme button, then the menu item. */
const pickTheme = (s, label) =>
  s.js(`(() => { document.querySelector('.topbar [aria-label="Theme"]').click(); const it = [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim() === ${JSON.stringify(label)}); it?.click(); return !!it; })()`);
const report = {};
const consoleLog = [];
const failures = [];
const check = (name, ok, detail) => {
  report[name] = detail ?? ok;
  if (!ok) failures.push(name);
};

mkdirSync(out, { recursive: true });
rmSync(data, { recursive: true, force: true });
rmSync(webview, { recursive: true, force: true });

/** Icon-only buttons without a name or tooltip; whether the top bar wraps. */
const AUDIT = `(() => {
  const unnamed = [...document.querySelectorAll("button")].filter((b) => b.offsetParent !== null && !b.textContent.trim()
    && !(b.getAttribute("aria-label") || "").trim()).map((b) => b.outerHTML.slice(0, 120));
  const untipped = [...document.querySelectorAll("button.ep-icon-button")].filter((b) => b.offsetParent !== null && !b.dataset.tip).map((b) => b.getAttribute("aria-label"));
  const bar = document.querySelector(".topbar");
  return { unnamed, untipped, topbar_height: bar ? bar.getBoundingClientRect().height : null, topbar_overflows: bar ? bar.scrollWidth > bar.clientWidth + 1 : null };
})()`;

const s = await launchApp({ exe, args: [paper], data, webview, port: 9337, out, consoleLog });
try {
  await s.until(`document.querySelectorAll(".pane.focused .page[data-page='1'] .text-layer span").length > 100`, 30000);
  await sleep(800);
  // A few marks and notes so the screens show them.
  await s.js(`(async () => {
    const call = (command, input) => window.__TAURI_INTERNALS__.invoke("bus_call", { command, input, deadlineMs: null, approved: [] });
    const p = (await call("workspace.list_papers", {}))[0];
    const now = Date.now();
    const base = { opacity: 1, width: 1.5, body: "", created_ms: now, modified_ms: now, category: null };
    const q = (x0, y0, x1, y1) => ({ page: 1, p: [{ x: x0, y: y0 }, { x: x1, y: y0 }, { x: x0, y: y1 }, { x: x1, y: y1 }] });
    await call("document.annotations_apply", { paper: p.id, ops: [
      { op: "add", annotation: { ...base, id: "ep-s1", page: 1, kind: "highlight", color: "#ffd400", rect: { page: 1, x0: 72, y0: 300, x1: 280, y1: 312 }, quads: [q(72, 300, 280, 312)], text: "sample", body: "Key definition of the method." } },
      { op: "add", annotation: { ...base, id: "ep-s2", page: 1, kind: "note", color: "#74c0fc", rect: { page: 1, x0: 300, y0: 420, x1: 320, y1: 440 }, body: "Compare with the baseline in Section 4." } },
    ] });
    await call("workspace.save_notes", { paper: p.id, text: "# Reading notes\\n\\n> The model is trained on simulated data.\\n> [p. 1](loc:1/300/ep-s1)\\n\\n- Check the **loss weights**\\n- Inline math $\\\\lambda = 0.1$" });
  })()`);
  await s.key("Escape");
  for (const [theme, label] of Object.entries(themes)) {
    await pickTheme(s, label);
    // Reading, with the notes panel open.
    await s.js(`(() => { const b = document.querySelector('.topbar [aria-label="Notes"]'); if (b && b.getAttribute("aria-pressed") !== "true") b.click(); })()`);
    await sleep(1200);
    await s.shot(`md-reading-${theme}.png`);
    if (theme === "light") report.audit_reading = await s.js(AUDIT);
  }
  // Dark app with pages in their original colours (theme and paper are separate choices).
  await pickTheme(s, "Dark");
  await s.js(`(() => { document.querySelector('.topbar [aria-label="Theme"]').click(); [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim() === "Original colours").click(); })()`);
  await sleep(1500);
  await s.shot("md-dark-app-original-paper.png");
  const truePages = await s.js(`({ paper: document.documentElement.dataset.paper, theme: document.documentElement.dataset.theme, images: document.querySelectorAll(".pane.focused img.page-canvas").length, recoloured: document.querySelectorAll(".pane.focused canvas.page-canvas").length })`);
  check("dark_app_true_colour_pages", truePages.theme === "dark" && truePages.paper === "original" && truePages.images > 0 && truePages.recoloured === 0, truePages);
  await s.js(`(() => { document.querySelector('.topbar [aria-label="Theme"]').click(); [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim() === "Match the app").click(); })()`);
  // Light: the tool flyout, the selection bar, a card, the margin view, focus mode, the library.
  await pickTheme(s, "Light");
  await s.js(`document.querySelector('.tool-rail [data-tool="highlight"]').click()`);
  await sleep(400);
  await s.shot("md-tool-flyout.png");
  await s.key("Escape");
  await s.js(`document.querySelector('.tool-rail [data-tool="select"]').click()`);
  await s.js(`(() => {
    const spans = [...document.querySelectorAll(".pane.focused .page[data-page='1'] .text-layer span")].slice(80, 88);
    const r = document.createRange(); r.setStartBefore(spans[0]); r.setEndAfter(spans.at(-1));
    const sel = getSelection(); sel.removeAllRanges(); sel.addRange(r);
    document.querySelector(".pane.focused .scroller").dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  })()`);
  await s.until(`document.querySelector(".selection-bar")`);
  await s.shot("md-selection-bar.png");
  report.audit_selection = await s.js(AUDIT);
  await s.js(`getSelection().removeAllRanges()`);
  await s.key("m", { ctrl: true, shift: true });
  await sleep(600);
  await s.shot("md-margin-view.png");
  await s.js(`document.querySelector(".margin-card")?.click()`);
  await sleep(500);
  await s.shot("md-card.png");
  report.audit_card = await s.js(AUDIT);
  await s.key("m", { ctrl: true, shift: true });
  await s.key("F11");
  await sleep(500);
  await s.shot("md-focus.png");
  await s.key("F11");
  await s.key("l", { ctrl: true });
  await sleep(600);
  await s.shot("md-library.png");
  report.audit_library = await s.js(AUDIT);
  await s.key("l", { ctrl: true });
  // The kit gallery.
  await s.js(`location.hash = "#gallery"; location.reload()`);
  await sleep(2500);
  await s.shot("md-gallery-light.png");
  await s.js(`document.querySelector('.ep-segmented [aria-label="dark"]').click()`);
  await sleep(300);
  await s.shot("md-gallery-dark.png");
  await s.js(`document.querySelector('.ep-segmented [aria-label="light"]').click(); location.hash = ""; location.reload()`);
  await sleep(1500);
} catch (e) {
  report.error = String(e?.stack ?? e);
  failures.push("exception");
} finally {
  s.ws.close();
  s.app.kill();
}

for (const k of Object.keys(report).filter((k) => k.startsWith("audit_"))) {
  const a = report[k];
  check(`${k}_named`, a && a.unnamed.length === 0 && a.untipped.length === 0, a);
}
check("topbar_one_row", report.audit_reading && report.audit_reading.topbar_overflows === false && report.audit_reading.topbar_height <= 41, report.audit_reading);
report.console = consoleLog;
report.failures = failures;
console.log(JSON.stringify(report, null, 2));
process.exit(failures.length === 0 ? 0 : 1);
