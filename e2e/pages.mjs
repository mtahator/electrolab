// End-to-end check that every page keeps its own size and direction (design language §9.11): when
// opened, after preparation, in split view, after closing and reopening, and in the thumbnails.
//
//   node e2e/pages.mjs <reader.exe> <scratch-dir> <out-dir> [other.pdf]

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out, other] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  const r = (report[name] ??= { ok: true, seen: [] });
  r.seen.push(detail);
  if (!ok) {
    r.ok = false;
    if (!failures.includes(name)) failures.push(name);
  }
};
mkdirSync(out, { recursive: true });
const data = join(scratch, "pages");
rmSync(data, { recursive: true, force: true });
const paper = other ?? join(root, "fixtures/documents/mixed-pages.pdf");
const s = await launchApp({ exe, args: [paper, join(root, "fixtures/documents/product-manual.pdf")], data, webview: join(scratch, "pages-webview"), port: 9398, out, consoleLog: [] });

/** Scroll through the focused pane; the shape of every page's box, its drawing and their fit. */
async function shapes() {
  const seen = {};
  const h = await s.js(`document.querySelector(".pane.focused .scroller").scrollHeight`);
  for (let y = 0; y < h; y += 420) {
    await s.js(`document.querySelector(".pane.focused .scroller").scrollTop = ${y}`);
    await sleep(220);
    for (const p of await s.js(`[...document.querySelectorAll(".pane.focused .page")].map((e) => { const r = e.getBoundingClientRect(), i = e.querySelector("img.page-canvas") ?? e.querySelector("img.page-preview"); return i?.naturalWidth ? { page: Number(e.dataset.page), w: r.width, h: r.height, iw: i.naturalWidth, ih: i.naturalHeight, bw: i.getBoundingClientRect().width, bh: i.getBoundingClientRect().height } : null; }).filter(Boolean)`)) seen[p.page] = p;
  }
  return seen;
}
const wrongIn = (seen, sizes) =>
  Object.values(seen)
    .filter((p) => {
      const want = sizes[p.page - 1].width / sizes[p.page - 1].height;
      const off = (x) => Math.abs(x / want - 1) > 0.02;
      return off(p.w / p.h) || off(p.iw / p.ih) || off(p.bw / p.bh);
    })
    .map((p) => `p${p.page} box ${Math.round(p.w)}x${Math.round(p.h)} drawing ${p.iw}x${p.ih}`);

try {
  await s.until(`document.querySelectorAll(".tab").length === 2 ? true : null`, 20000);
  await s.js(`document.querySelectorAll(".tab")[0].click()`);
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 20000);
  const papers = await s.call("workspace.list_papers", { limit: 5, offset: 0, sort: "added" });
  const name = paper.split(/[\\/]/).pop();
  const file = papers.find((p) => p.original_name === name).file;
  // The truth: the size of the drawing PDFium makes.
  const info = await s.call("document.info", { file });
  const sizes = [];
  for (let p = 1; p <= info.pages; p++) {
    const r = await s.call("document.render", { file, page: p, width_px: 600 });
    sizes.push({ width: r.width_px, height: r.height_px });
  }
  const step = async (when) => {
    const seen = await shapes();
    const wrong = wrongIn(seen, sizes);
    check("pages_keep_their_own_shape", wrong.length === 0 && Object.keys(seen).length === info.pages, { when, pages: Object.keys(seen).length, wrong });
    // One scale: the same number of pixels per point on every page.
    const now = (await s.call("document.info", { file })).sizes;
    const scales = Object.values(seen).map((p) => p.w / now[p.page - 1].width);
    check("one_scale_for_all_pages", Math.max(...scales) / Math.min(...scales) < 1.01, { when, scales: [Math.min(...scales), Math.max(...scales)] });
  };
  await step("opened");
  await s.shot("pages-opened.png");
  await s.call("document.prepare", { file });
  await step("prepared");
  await s.js(`document.querySelector('.topbar [aria-label^="Split view"]').click()`);
  await sleep(1500);
  await step("split view");
  await s.js(`document.querySelector('.topbar [aria-label^="Split view"]').click()`);
  await sleep(800);
  await s.js(`(() => { const t = [...document.querySelectorAll(".tab")][0]; t.querySelector('[aria-label^="Close"]').click(); })()`);
  await sleep(800);
  await s.js(`document.querySelector('.topbar [aria-label^="Library"]').click()`);
  await s.until(`document.querySelector(".library-view")`, 5000);
  const title = await s.js(`(() => { const e = [...document.querySelectorAll(".library-view .paper-row, .library-view .paper-cover, .library-view li, .library-view tr")].find((x) => /different sizes|Renewable|mixed/i.test(x.textContent)); const b = e?.querySelector("button, a") ?? e; b?.dispatchEvent(new MouseEvent("click", { bubbles: true })); b?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return e?.textContent.slice(0, 60); })()`);
  await s.until(`document.querySelector(".pane.focused .page")`, 8000);
  await sleep(1200);
  const reopened = await s.js(`document.querySelector(".pane.focused .tab.active, .tab.active")?.textContent ?? ""`);
  await step(`reopened (${title ?? "?"} / ${reopened})`);
  // Thumbnails.
  await s.js(`(() => { if (!document.querySelector(".side.left")) document.querySelector('.topbar [aria-label^="Side panel"]').click(); })()`);
  await s.js(`document.querySelectorAll(".side.left .ep-panel button.ep-icon-button")[1].click()`);
  await s.until(`document.querySelectorAll(".thumb img").length >= Math.min(4, ${info.pages}) ? true : null`, 10000);
  await sleep(800);
  const thumbs = await s.js(`[...document.querySelectorAll(".thumb")].map((t) => { const i = t.querySelector("img"); const r = i?.getBoundingClientRect(); return i?.naturalWidth ? { page: Number(t.dataset.page), w: r.width, h: r.height, iw: i.naturalWidth, ih: i.naturalHeight, bw: r.width, bh: r.height } : null; }).filter(Boolean)`);
  check("thumbnails_keep_the_shape", thumbs.length >= 4 && wrongIn(thumbs, sizes).length === 0, { thumbs: thumbs.length, wrong: wrongIn(thumbs, sizes) });
  await s.shot("pages-thumbs.png");
} finally {
  s.app.kill();
}
writeFileSync(join(out, "pages.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
