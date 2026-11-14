// End-to-end check of document intelligence (Milestone 4) on the real, built app.
//
//   EP_MODELS_DIR=<folder with egret-medium/model.onnx> node e2e/intel.mjs <reader.exe> <scratch-dir> <out-dir>
//
// With the layout model: background preparation finishes, the Contents panel lists figures,
// tables and equations, a figure mention opens its figure in place and "Go to" + Back work,
// a citation marker shows its reference, Ctrl+click picks a figure, a table opens as cells.
// Without the model (a second run with an empty models folder): the status line offers it.

import { mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const paper = join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf");
const models = process.env.EP_MODELS_DIR;
const report = {};
const consoleLog = [];
const failures = [];
const check = (name, ok, detail) => {
  report[name] = detail ?? ok;
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });

async function run(tag, modelsDir, body) {
  const data = join(scratch, `intel-${tag}`);
  const webview = join(scratch, `intel-${tag}-webview`);
  rmSync(data, { recursive: true, force: true });
  rmSync(webview, { recursive: true, force: true });
  const prev = process.env.EP_MODELS_DIR;
  process.env.EP_MODELS_DIR = modelsDir;
  const s = await launchApp({ exe, args: [paper], data, webview, port: 9338, out, consoleLog });
  process.env.EP_MODELS_DIR = prev;
  try {
    await s.until(`document.querySelectorAll(".pane.focused .page[data-page='1'] .text-layer span").length > 100`, 30000);
    await body(s);
  } catch (e) {
    report[`${tag}_error`] = String(e?.stack ?? e);
    failures.push(`${tag}_exception`);
  } finally {
    s.ws.close();
    s.app.kill();
    await sleep(1000);
  }
}

const page = (s) => s.js(`document.querySelector(".page-box").value`);

if (models) {
  await run("model", models, async (s) => {
    // Preparation runs in the background and finishes.
    const t0 = Date.now();
    const done = await s.until(
      `window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] })
        .then((p) => window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.preparation", input: { file: p[0].file }, deadlineMs: null, approved: [] }))
        .then((r) => r.stages.find((x) => x.stage === "objects").status === "done" ? r.stages.find((x) => x.stage === "objects").total : null)`,
      90000,
    );
    check("preparation_done", !!done, { pages: done, seconds: Math.round((Date.now() - t0) / 1000) });

    // Contents: jump lists.
    await s.js(`(() => { if (!document.querySelector(".side.left")) document.querySelector('.topbar [aria-label="Side panel"]').click(); })()`);
    await s.js(`document.querySelector('.side.left .ep-panel-tabs [aria-label="Contents"]')?.click()`);
    const heads = await s.until(`(() => { const h = [...document.querySelectorAll(".jumps .jump-head")]; return h.length >= 2 ? h.map((x) => x.textContent) : null; })()`, 15000);
    await s.js(`[...document.querySelectorAll(".jumps .jump-head")][0].click()`);
    await s.until(`document.querySelectorAll(".jumps .jump-item").length`);
    await s.js(`[...document.querySelectorAll(".jumps .jump-item")].at(-1).click()`);
    const jumped = await s.until(`(() => { const v = document.querySelector(".page-box").value; return v !== "1" ? v : null; })()`);
    check("contents_jump_lists", !!heads && !!jumped, { heads, jumped });
    await s.shot("m4-contents.png");

    // A figure mention: popover with the figure; Go to; Back.
    await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "2"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
    const mention = await s.until(`(() => { const m = [...document.querySelectorAll(".pane.focused .mention:not(.citation)")]; return m.length || null; })()`, 20000);
    await s.js(`[...document.querySelectorAll(".pane.focused .mention:not(.citation)")][0].click()`);
    const img = await s.until(`(() => { const i = document.querySelector(".peek .peek-image"); return i && i.complete && i.naturalWidth > 50 ? i.naturalWidth : null; })()`, 15000);
    const title = await s.js(`document.querySelector(".peek .ep-card-head strong")?.textContent`);
    await s.shot("m4-mention-peek.png");
    const before = await page(s);
    await s.js(`[...document.querySelectorAll(".peek button")].find((b) => b.textContent.trim() === "Go to")?.click()`);
    const moved = await s.until(`(() => { const v = document.querySelector(".page-box").value; return v !== ${JSON.stringify(before)} ? v : null; })()`, 5000);
    await s.scrollerKey("ArrowLeft", { alt: true });
    const backTo = await s.until(`document.querySelector(".page-box").value === ${JSON.stringify(before)} ? true : null`, 5000);
    check("mention_popover_go_back", !!mention && !!img && /^(Figure|Table|Equation|Section) /.test(title ?? "") && !!backTo, { mention, img, title, before, moved, backTo });

    // A citation marker shows its reference.
    await s.js(`[...document.querySelectorAll(".pane.focused .mention.citation")][0]?.click()`);
    const ref = await s.until(`document.querySelector(".peek .peek-refs li p")?.textContent || null`, 10000);
    check("citation_popover", !!ref && ref.length > 20, ref?.slice(0, 80));
    await s.key("Escape");

    // Ctrl+click on a figure picks it; a table opens as cells.
    const figs = await s.js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] })
      .then((p) => window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.contents", input: { file: p[0].file }, deadlineMs: null, approved: [] }))`);
    // Page width in points from the document info, to convert the object's centre to the screen.
    const info = await s.js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] })
      .then((p) => window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.info", input: { file: p[0].file }, deadlineMs: null, approved: [] }))`);
    const ctrlClick = async (o) => {
      await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "${o.rect.page}"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
      await s.until(`document.querySelector(".pane.focused .page[data-page='${o.rect.page}'] .region-host")`);
      await sleep(500);
      const w = info.sizes[o.rect.page - 1].width;
      return s.js(`(() => {
        const page = document.querySelector(".pane.focused .page[data-page='${o.rect.page}']");
        const r = page.getBoundingClientRect();
        const k = r.width / ${w};
        const at = { clientX: r.left + ${(o.rect.x0 + o.rect.x1) / 2} * k, clientY: r.top + ${(o.rect.y0 + o.rect.y1) / 2} * k, bubbles: true, button: 0, ctrlKey: true, pointerId: 1 };
        page.dispatchEvent(new PointerEvent("pointerdown", at));
        window.dispatchEvent(new PointerEvent("pointerup", at));
        return true;
      })()`);
    };
    await ctrlClick(figs.figures[0]);
    const picked = await s.until(`(() => { const t = document.querySelector(".peek .ep-card-head strong")?.textContent; const i = document.querySelector(".peek .peek-image"); return t && i && i.naturalWidth > 50 ? t : null; })()`, 15000);
    check("region_pick_figure", picked === figs.figures[0].label, { picked, expected: figs.figures[0].label });
    await s.key("Escape");
    if (figs.tables[0]) {
      await ctrlClick(figs.tables[0]);
      await s.until(`document.querySelector('.peek [aria-label="Open as table"]')`, 15000);
      await s.js(`document.querySelector('.peek [aria-label="Open as table"]').click()`);
      const rows = await s.until(`document.querySelectorAll(".table-view tbody tr").length || null`, 10000);
      await s.shot("m4-table.png");
      check("table_view", rows >= 2, rows);
      await s.js(`document.querySelector('.table-view [aria-label="Close"]').click()`);
    }
  });
} else {
  report.note = "EP_MODELS_DIR not set: the layout-model part was skipped";
}

// Without the model: the status line offers it; the dialog explains it; nothing downloads.
const empty = join(scratch, "intel-no-models");
rmSync(empty, { recursive: true, force: true });
mkdirSync(empty, { recursive: true });
await run("nomodel", empty, async (s) => {
  const offer = await s.until(`[...document.querySelectorAll(".ep-status .ep-status-item")].find((b) => b.textContent.includes("Recognise figures"))?.textContent || null`, 15000);
  await s.js(`[...document.querySelectorAll(".ep-status .ep-status-item")].find((b) => b.textContent.includes("Recognise figures")).click()`);
  const dialog = await s.until(`document.querySelector('.ep-dialog[aria-label="Download the layout model?"]') ? true : null`);
  await s.shot("m4-model-dialog.png");
  await s.js(`[...document.querySelectorAll(".ep-dialog button")].find((b) => b.textContent.trim() === "Not now").click()`);
  // Text-based links still work without the model.
  await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "2"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
  const citations = await s.until(`document.querySelectorAll(".pane.focused .mention.citation").length || null`, 15000);
  check("no_model_offer_and_text_links", !!offer && !!dialog && citations > 0, { offer, dialog, citations });
});

report.console = consoleLog;
report.failures = failures;
console.log(JSON.stringify(report, null, 2));
process.exit(failures.length === 0 ? 0 : 1);
