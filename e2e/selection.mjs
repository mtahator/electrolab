// End-to-end check of selecting text (design language §9.10), with real mouse input.
//
//   node e2e/selection.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const papers = { paper: join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf"), manual: join(root, "fixtures/documents/product-manual.pdf") };
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok: (report[name]?.ok ?? true) && ok, ...report[name], ...detail, };
  if (!ok && !failures.includes(name)) failures.push(name);
};
mkdirSync(out, { recursive: true });

// In the page: the visible words of page `p` whose printed box and span are compared.
const FIT = `(() => {
  const bad = []; let n = 0;
  for (const s of document.querySelectorAll(".pane.focused .text-layer span[data-page]")) {
    const w = window.__words[s.dataset.page]?.find((w) => w.start === Number(s.dataset.start));
    if (!w) continue;
    const page = s.closest(".page").getBoundingClientRect();
    const k = page.width / window.__sizes[s.dataset.page];
    const r = s.getBoundingClientRect();
    n++;
    const err = Math.max(Math.abs(r.left - page.left - w.rect.x0 * k), Math.abs(r.right - page.left - w.rect.x1 * k), Math.abs(r.height - (w.rect.y1 - w.rect.y0) * k));
    if (err > 1.5) bad.push(s.textContent + ":" + err.toFixed(1));
  }
  return { n, bad: bad.length, sample: bad.slice(0, 5) };
})()`;

for (const [name, path] of Object.entries(papers)) {
  const data = join(scratch, `selection-${name}`);
  const webview = join(scratch, `selection-${name}-webview`);
  rmSync(data, { recursive: true, force: true });
  rmSync(webview, { recursive: true, force: true });
  const s = await launchApp({ exe, args: [path], data, webview, port: name === "paper" ? 9360 : 9361, out, consoleLog: [] });
  const mouse = (type, x, y, n = 1) => s.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: n });
  try {
    await s.until(`document.querySelectorAll(".pane.focused .text-layer span[data-page]").length > 15 ? true : null`, 20000);
    const file = (await s.call("workspace.list_papers", { limit: 5, offset: 0, sort: "added" }))[0].file;
    const info = await s.call("document.info", { file });
    const text = await s.call("document.text", { file, page: 1 });
    await s.js(`window.__words = { 1: ${JSON.stringify(text.words)} }; window.__sizes = { 1: ${JSON.stringify(info.sizes?.[0]?.width ?? info.page_sizes?.[0]?.width ?? null)} }`);
    if (!(await s.js(`window.__sizes[1]`))) await s.js(`window.__sizes[1] = ${JSON.stringify(findWidth(info))}`);
    const fit = await s.js(FIT);
    check("words_fit_the_print", fit.n > 10 && fit.bad <= fit.n * 0.01, { [`${name}_fit`]: fit });

    // The pointer over the middle of a printed letter finds that letter (or a neighbour).
    const hit = await s.js(`(() => {
      const boxes = ${JSON.stringify(text.boxes)}; const chars = Array.from(${JSON.stringify(text.text)});
      const page = document.querySelector('.pane.focused .page[data-page="1"]').getBoundingClientRect();
      const k = page.width / window.__sizes[1]; let n = 0, good = 0;
      for (let i = 0; i < boxes.length; i++) {
        const b = boxes[i]; if (!b || !/[A-Za-z0-9]/.test(chars[i])) continue;
        const x = page.left + (b[0] + b[2]) / 2 * k, y = page.top + (b[1] + b[3]) / 2 * k;
        if (y < 130 || y > innerHeight - 60) continue;
        const r = document.caretRangeFromPoint(x, y);
        const span = r?.startContainer.parentElement?.closest("span[data-page]");
        n++;
        if (span && Math.abs(Number(span.dataset.start) + r.startOffset - i) <= 1) good++;
      }
      return { n, good };
    })()`);
    check("pointer_finds_the_letter", hit.n > 20 && hit.good >= hit.n * 0.97, { [`${name}_hits`]: hit });

    // Drag from the middle of one word to the middle of another on the same line.
    const line = text.lines.map((l) => ({ ...l, words: text.words.filter((w) => w.start >= l.start && w.end <= l.end && w.end - w.start >= 3) })).filter((l) => l.words.length >= 2).sort((a, b) => (b.rect.y1 - b.rect.y0) - (a.rect.y1 - a.rect.y0))[0];
    const [w1, w2] = [line.words[0], line.words[line.words.length - 1]];
    const at = await s.js(`(() => { const p = document.querySelector('.pane.focused .page[data-page="1"]').getBoundingClientRect(); return { l: p.left, t: p.top, k: p.width / window.__sizes[1] }; })()`);
    const pt = (i, f) => { const b = text.boxes[i]; return { x: at.l + (b[0] + (b[2] - b[0]) * f) * at.k, y: at.t + ((b[1] + b[3]) / 2) * at.k }; };
    const from = pt(w1.start, 0.15);
    const to = pt(w2.end - 2, 0.9);
    await mouse("mousePressed", from.x, from.y);
    for (let i = 1; i <= 8; i++) await mouse("mouseMoved", from.x + ((to.x - from.x) * i) / 8, from.y + ((to.y - from.y) * i) / 8);
    await mouse("mouseReleased", to.x, to.y);
    await sleep(250);
    const got = await s.js(`(() => {
      const rows = [...document.querySelectorAll(".pane.focused .selection-row")].map((r) => r.getBoundingClientRect()).map((r) => [r.left, r.top, r.right, r.bottom]);
      const dt = new DataTransfer(); const ev = new ClipboardEvent("copy", { clipboardData: dt, bubbles: true, cancelable: true });
      document.querySelector(".pane.focused .scroller").dispatchEvent(ev);
      return { rows, copied: dt.getData("text/plain") };
    })()`);
    await s.shot(`selection-${name}.png`);
    const chars = Array.from(text.text);
    const squash = (t) => t.replace(/\s+/g, " ").trim();
    // The characters under the band are the ones copied.
    const row = got.rows[0] ?? [0, 0, 0, 0];
    const under = chars.filter((c, i) => {
      const b = text.boxes[i];
      if (!b || i < line.start || i >= line.end || /\s/.test(c)) return false;
      const x = at.l + ((b[0] + b[2]) / 2) * at.k;
      return x > row[0] && x < row[2];
    }).join("");
    const wide = Math.abs(row[0] - from.x) < 12 * at.k && Math.abs(row[2] - to.x) < 12 * at.k;
    check("blue_is_what_is_copied", got.rows.length === 1 && wide && squash(got.copied).replace(/ /g, "") === under, { [`${name}_selection`]: { copied: got.copied, under, rows: got.rows.length, wide } });

    // Scrolling (and a window that gets a little smaller) keeps the pages, and so what is selected.
    const before = await s.js(`(window.__page = document.querySelector('.pane.focused .page[data-page="1"]'), getSelection().toString())`);
    await s.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: from.x, y: from.y + 60, deltaX: 0, deltaY: 90 });
    await sleep(500);
    const moved = await s.js(`document.querySelector(".pane.focused .scroller").scrollTop`);
    const size = await s.js(`({ w: innerWidth, h: innerHeight })`);
    await s.send("Emulation.setDeviceMetricsOverride", { width: size.w - 60, height: size.h - 40, deviceScaleFactor: 0, mobile: false });
    await sleep(500);
    const kept = await s.js(`({ same: window.__page.isConnected, selected: getSelection().toString() })`);
    check("a_selection_survives_scrolling", moved > 20 && before.length > 3 && kept.same === true && kept.selected === before, { [`${name}_scrolled`]: { moved, before, ...kept } });
    await s.send("Emulation.clearDeviceMetricsOverride", {});
    await s.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: from.x, y: from.y + 60, deltaX: 0, deltaY: -400 });
    await sleep(500);

    // After zooming twice the words still fit (heights were once left behind).
    for (let i = 0; i < 2; i++) await s.js(`document.querySelector('.topbar [aria-label^="Zoom in"]').click()`);
    await sleep(900);
    const after = await s.js(FIT);
    const wider = await s.js(`document.querySelector('.pane.focused .page[data-page="1"]').getBoundingClientRect().width / window.__sizes[1]`);
    check("words_fit_after_zoom", wider > at.k * 1.05 && after.n > 5 && after.bad <= after.n * 0.01, { [`${name}_zoomed`]: { ...after, from: at.k, to: wider } });
  } finally {
    s.app.kill();
    await sleep(500);
  }
}

/** The first page width anywhere in `document.info`. */
function findWidth(v) {
  if (v && typeof v === "object") {
    if (typeof v.width === "number" && v.width > 50) return v.width;
    for (const x of Object.values(v)) {
      const w = findWidth(x);
      if (w) return w;
    }
  }
  return null;
}

writeFileSync(join(out, "selection.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
