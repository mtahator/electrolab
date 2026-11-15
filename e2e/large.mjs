// End-to-end check of a large document under load (design language §17, review 2026-11-15).
//
//   node e2e/large.mjs <reader.exe> <scratch-dir> <out-dir>
//   EP_LARGE_PDF=<any large PDF> node e2e/large.mjs …   (default: fixtures/documents/book-800.pdf)
//
// The book is opened fresh, so preparation, previews and the search index all run in the
// background while every step below is measured. Gestures use real input (DevTools `Input.*`),
// as a user would make them. Budgets are those of design language §17.3.

import { mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const book = process.env.EP_LARGE_PDF ?? join(root, "fixtures/documents/book-800.pdf");
const report = { book };
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
const data = join(scratch, "large");
const webview = join(scratch, "large-webview");
rmSync(data, { recursive: true, force: true });
rmSync(webview, { recursive: true, force: true });

const t0 = Date.now();
const s = await launchApp({ exe, args: [book], data, webview, port: 9339, out, consoleLog: [] });
const key = (k, code, vk, modifiers = 0, text) =>
  s
    .send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", key: k, code, windowsVirtualKeyCode: vk, modifiers, ...(text ? { text } : {}) })
    .then(() => s.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk, modifiers }));
const timeUntil = async (expr, ms) => {
  const t = Date.now();
  const v = await s.until(expr, ms).catch(() => null);
  return { value: v, ms: Date.now() - t };
};

try {
  // 1. First picture and selectable text of the first page.
  const first = await timeUntil(`document.querySelector(".pane.focused .page[data-page='1'] img.page-canvas, .pane.focused .page[data-page='1'] img.page-preview") ? true : null`, 20000);
  const firstMs = Date.now() - t0;
  const appReady = firstMs - first.ms;
  check("first_picture", first.value && first.ms < 1000, { ms_after_window: first.ms, ms_from_launch: firstMs });
  let text = await timeUntil(`[...document.querySelectorAll(".pane.focused .page")].some((p) => p.querySelectorAll(".text-layer span").length > 20) ? true : null`, 2500);
  if (!text.value) {
    // The opening pages have almost no text (a cover): measure on a text page after a jump.
    await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "40"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
    text = await timeUntil(`document.querySelectorAll(".pane.focused .page[data-page='40'] .text-layer span").length > 20 ? true : null`, 20000);
    text.ms -= first.ms;
    await sleep(300);
  }
  // The page with text that is in view (a book may open on a cover without text).
  const textPage = await s.js(`[...document.querySelectorAll(".pane.focused .page")].find((p) => { const b = p.getBoundingClientRect(); return b.bottom > 0 && b.top < innerHeight && p.querySelectorAll(".text-layer span").length > 20; })?.dataset.page ?? "1"`);
  check("text_selectable", text.value && text.ms + first.ms < 1500, { ms_after_window: text.ms + first.ms, app_start_ms: appReady });

  // Nothing covers the text: what is under the pointer over a word is that word (the link layer
  // once covered whole pages, so nothing could be selected or highlighted).
  const covered = await s.js(`(() => { const spans = [...document.querySelectorAll(".pane.focused .page .text-layer span")].filter((x) => { const b = x.getBoundingClientRect(); return b.width > 30 && b.top > 60 && b.bottom < innerHeight - 40 && b.left > 0 && b.right < innerWidth; }).slice(0, 20); const bad = spans.filter((sp) => { const b = sp.getBoundingClientRect(); const e = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2); return e !== sp; }).map((sp) => document.elementFromPoint(sp.getBoundingClientRect().left + 2, sp.getBoundingClientRect().top + 2)?.className); return { checked: spans.length, covered_by: bad }; })()`);
  check("text_not_covered", covered.checked > 5 && covered.covered_by.length === 0, covered);

  // 2. Highlight with the tool, dragging over text with the mouse.
  await s.js(`document.querySelector(".pane.focused .scroller").focus()`);
  await key("h", "KeyH", 72, 0, "h");
  const span = await s.js(`(() => { const sp = [...document.querySelectorAll(".pane.focused .page .text-layer span")].find((x) => { const b = x.getBoundingClientRect(); return b.top > 120 && b.bottom < innerHeight - 80 && b.width > 20 && b.left < innerWidth / 2; }); if (!sp) return null; const b = sp.getBoundingClientRect(); return { x0: b.left + 2, x1: Math.min(innerWidth - 60, b.left + 220), y: b.top + b.height / 2 }; })()`);
  let highlighted = 0;
  if (span) {
    await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: span.x0, y: span.y });
    await s.send("Input.dispatchMouseEvent", { type: "mousePressed", x: span.x0, y: span.y, button: "left", clickCount: 1 });
    for (let k = 1; k <= 10; k++) await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: span.x0 + ((span.x1 - span.x0) * k) / 10, y: span.y, button: "left", buttons: 1 });
    await s.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: span.x1, y: span.y, button: "left", clickCount: 1 });
    highlighted = await s.until(`(async () => { const p = await window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] }); const set = await window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.annotations", input: { paper: p[0].id, file: p[0].file }, deadlineMs: null, approved: [] }); return set.annotations.filter((a) => a.kind === "highlight").length || null; })()`, 5000).catch(() => 0);
  }
  const hint = await s.js(`[...document.querySelectorAll(".ep-status .ep-status-item")].map((x) => x.textContent).find((x) => x.includes("Esc")) ?? null`);
  check("highlight_by_dragging", highlighted > 0 && !!hint, { highlights: highlighted, status_hint: hint });
  await key("Escape", "Escape", 27);

  // 3. Find bar: answers at once, the count grows while searching, Enter steps.
  await key("f", "KeyF", 70, 2);
  const bar = await timeUntil(`document.querySelector(".find-bar input") ? true : null`, 3000);
  const tType = Date.now();
  await key("a", "KeyA", 65, 0, "a");
  const firstHit = await timeUntil(`(() => { const c = document.querySelector(".find-bar-count")?.textContent ?? ""; const m = c.match(/\\/\\s*(\\d+)/); return m && Number(m[1]) > 0 ? Number(m[1]) : null; })()`, 15000);
  const counts = [];
  for (let k = 0; k < 6; k++) {
    counts.push(await s.js(`Number((document.querySelector(".find-bar-count")?.textContent ?? "").match(/\\/\\s*(\\d+)/)?.[1] ?? 0)`));
    await sleep(250);
  }
  const done = await timeUntil(`document.querySelector(".find-bar .spinner") ? null : true`, 60000);
  const total = await s.js(`Number((document.querySelector(".find-bar-count")?.textContent ?? "").match(/\\/\\s*(\\d+)/)?.[1] ?? 0)`);
  await key("Enter", "Enter", 13, 0, "\r");
  const stepped = await s.js(`document.querySelector(".find-bar-count")?.textContent`);
  await s.shot("large-find-bar.png");
  const grew = counts.some((c, i) => i > 0 && c > counts[i - 1]) || counts[0] === total;
  check("find_streams", !!bar.value && firstHit.value && Date.now() - tType > 0 && firstHit.ms < 1000 && grew && total > 100, { first_hit_ms: firstHit.ms, counts, total, all_searched_ms: firstHit.ms + done.ms + 1500, after_enter: stepped });
  await key("Escape", "Escape", 27);

  // 4. Jump far: sharp page within the budget.
  const target = await s.js(`Math.round(Number(document.querySelector(".page-box").parentElement.textContent.match(/\\/\\s*(\\d+)/)[1]) / 2)`);
  await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "${target}"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
  const sharp = await timeUntil(`document.querySelector(".pane.focused .page[data-page='${target}'] img.page-canvas")?.complete ? true : null`, 10000);
  check("sharp_after_jump", sharp.value && sharp.ms < 800, { page: target, ms: sharp.ms });

  // 5. Page indicator while scrolling.
  const mid = await s.js(`(() => { const b = document.querySelector(".pane.focused .scroller").getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
  await s.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: mid.x, y: mid.y, deltaX: 0, deltaY: 400 });
  const indicator = await timeUntil(`document.querySelector(".page-indicator")?.textContent || null`, 1500);
  const gone = await timeUntil(`document.querySelector(".page-indicator") ? null : true`, 3000);
  check("page_indicator", !!indicator.value && !!gone.value, { shown: indicator.value, hidden_after_ms: gone.ms });

  // 6. Ctrl + wheel zooms around the pointer: the point under the pointer stays under it.
  const at = { x: mid.x - 120, y: mid.y + 60 };
  const under = (p) =>
    s.js(`(() => { const el = document.elementFromPoint(${p.x}, ${p.y})?.closest(".page"); if (!el) return null; const r = el.getBoundingClientRect(); return { page: el.dataset.page, fx: (${p.x} - r.left) / r.width, fy: (${p.y} - r.top) / r.height }; })()`);
  const before = await under(at);
  const zoomBefore = await s.js(`document.querySelector(".zoom-button")?.textContent`);
  for (let k = 0; k < 4; k++) await s.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: at.x, y: at.y, deltaX: 0, deltaY: -100, modifiers: 2 });
  await sleep(600);
  const after = await under(at);
  const zoomAfter = await s.js(`document.querySelector(".zoom-button")?.textContent`);
  const drift = before && after && before.page === after.page ? Math.hypot(before.fx - after.fx, before.fy - after.fy) : 1;
  check("wheel_zoom_at_pointer", zoomBefore !== zoomAfter && drift < 0.02, { zoomBefore, zoomAfter, before, after, drift: Math.round(drift * 1000) / 1000 });
  await key("0", "Digit0", 48, 2);

  // 7. A hover popover closes when the page scrolls.
  await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "6"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
  const mention = await s.until(`(() => { const m = [...document.querySelectorAll(".pane.focused .mention:not(.citation)")].find((x) => { const b = x.getBoundingClientRect(); return b.top > 100 && b.bottom < innerHeight - 100; }); if (!m) return null; const b = m.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`, 30000).catch(() => null);
  await sleep(1000); // the jump has settled: scrolling closes hover popovers by design
  let popover = null;
  let closed = null;
  if (mention) {
    // Come from elsewhere, as a hand does, so the pointer enters the mention.
    await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: mention.x - 200, y: mention.y + 200 });
    await sleep(100);
    await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: mention.x, y: mention.y });
    popover = await s.until(`document.querySelector(".peek") ? true : null`, 5000).catch(() => null);
    await s.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: mid.x, y: mid.y, deltaX: 0, deltaY: 200 });
    closed = await s.until(`document.querySelector(".peek") ? null : true`, 2000).catch(() => null);
  }
  // A book given from outside may have no figure mention on that page: then there is nothing to check.
  const noMention = !mention && !!process.env.EP_LARGE_PDF;
  check("hover_popover_closes_on_scroll", noMention || (!!popover && !!closed), { mention: !!mention, opened: !!popover, closed: !!closed, ...(noMention ? { note: "no mention on the page" } : {}) });

  // 8. Split view: the panes can be resized by dragging.
  await s.js(`document.querySelector('.topbar [aria-label="Split view"]')?.click()`);
  const handle = await s.until(`(() => { const h = document.querySelector(".pane-resizer"); if (!h) return null; const b = h.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`, 5000).catch(() => null);
  let widths = null;
  if (handle) {
    const w0 = await s.js(`[...document.querySelectorAll(".pane")].map((p) => Math.round(p.getBoundingClientRect().width))`);
    await s.send("Input.dispatchMouseEvent", { type: "mousePressed", x: handle.x, y: handle.y, button: "left", clickCount: 1 });
    for (let k = 1; k <= 5; k++) await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: handle.x - 30 * k, y: handle.y, button: "left", buttons: 1 });
    await s.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: handle.x - 150, y: handle.y, button: "left", clickCount: 1 });
    await sleep(200);
    widths = { before: w0, after: await s.js(`[...document.querySelectorAll(".pane")].map((p) => Math.round(p.getBoundingClientRect().width))`) };
  }
  check("split_resizable", !!widths && widths.after[0] < widths.before[0] - 100, widths ?? {});
  await s.shot("large-split.png");
} catch (e) {
  report.error = String(e?.stack ?? e);
  failures.push("exception");
} finally {
  s.ws.close();
  s.app.kill();
}

report.failures = failures;
console.log(JSON.stringify(report, null, 2));
process.exit(failures.length === 0 ? 0 : 1);
