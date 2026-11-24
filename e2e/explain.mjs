// End-to-end check of Explain (P1.M5.UI.1, design language §11), with real mouse input.
//
//   node e2e/explain.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Without a model: a double-click opens nothing; the selection bar's Explain says what is missing.
// With a model (EP_LLM_DIR names a folder with a known GGUF): a double-click streams an answer,
// the check ends in words with page chips, Stop and Esc work, the model process ends with the app.

import { execSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const paper = join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf");
const llm = process.env.EP_LLM_DIR;
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });

async function open(name, port) {
  const data = join(scratch, name);
  const webview = join(scratch, `${name}-webview`);
  rmSync(data, { recursive: true, force: true });
  rmSync(webview, { recursive: true, force: true });
  const s = await launchApp({ exe, args: [paper], data, webview, port, out, consoleLog: [] });
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 20000);
  await s.until(`document.querySelectorAll(".pane.focused .text-layer span[data-page]").length > 50 ? true : null`, 20000);
  return s;
}
const mouse = (s, type, x, y, clicks) => s.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: clicks });
async function doubleClick(s, x, y) {
  for (const n of [1, 2]) {
    await mouse(s, "mousePressed", x, y, n);
    await mouse(s, "mouseReleased", x, y, n);
  }
}
/** A word on the screen that contains `re`. */
const wordAt = (s, re) =>
  s.until(`(() => {
    for (const e of document.querySelectorAll(".pane.focused .text-layer span[data-page]")) {
      if (!${re}.test(e.textContent)) continue;
      const r = e.getBoundingClientRect();
      if (r.width < 8 || r.top < 120 || r.bottom > innerHeight - 320) continue;
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, text: e.textContent };
    }
    return null;
  })()`, 8000);
const esc = async (s) => {
  for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
};

// ---- no model ---------------------------------------------------------------------------------
delete process.env.EP_LLM_DIR;
{
  const s = await open("explain-none", 9350);
  try {
    const w = await wordAt(s, "/^\\s*(learning|control|models?)\\s*$/i");
    await doubleClick(s, w.x, w.y);
    await sleep(600);
    const opened = await s.js(`!!document.querySelector(".explain")`);
    await s.js(`document.querySelector('.selection-bar [aria-label^="Explain"]').click()`);
    const says = await s.until(`document.querySelector(".explain-failed p")?.textContent ?? null`, 8000);
    await s.shot("explain-no-model.png");
    check("no_model_is_quiet_then_clear", !opened && /No language model/.test(says ?? ""), { opened, says });
    await esc(s);
    check("esc_closes", (await s.until(`document.querySelector(".explain") ? null : true`, 2000)) === true);
  } finally {
    s.app.kill();
  }
}

// ---- with a model -----------------------------------------------------------------------------
if (llm) {
  let llmPid = 0;
  process.env.EP_LLM_DIR = llm;
  const s = await open("explain-model", 9351);
  try {
    await s.until(`document.querySelector(".pane.focused .scroller")`);
    const w = await wordAt(s, "/^\\s*(learning|reinforcement|control|optimization|models?)\\s*$/i");
    const t0 = Date.now();
    await doubleClick(s, w.x, w.y);
    const card = await s.until(`document.querySelector(".explain") ? true : null`, 2000);
    const shownMs = Date.now() - t0;
    const first = await s.until(`(document.querySelector(".explain-text")?.textContent ?? "").length > 5 ? true : null`, 120000);
    const firstMs = Date.now() - t0;
    const done = await s.until(`(() => { const e = document.querySelector(".explain-status"); return e && !e.querySelector(".spinner") ? e.textContent : null; })()`, 120000);
    const doneMs = Date.now() - t0;
    const text = await s.js(`document.querySelector(".explain-text").textContent`);
    const pages = await s.js(`[...document.querySelectorAll(".explain-evidence .loc-chip")].map((c) => c.textContent)`);
    check("one_chip_per_page", new Set(pages).size === pages.length, { pages });
    const chips = await s.js(`document.querySelectorAll(".explain-evidence .loc-chip").length`);
    const covers = await s.js(`(() => { const c = document.querySelector(".explain").getBoundingClientRect(); return c.top < ${w.y} && c.bottom > ${w.y} && c.left < ${w.x} && c.right > ${w.x}; })()`);
    await s.shot("explain-answer.png");
    check("answer_streams_and_is_checked", !!card && !!first && text.length > 40 && /Checked|Corrected|Not checked/.test(done ?? ""), { word: w.text, shownMs, firstMs, doneMs, done, chips, text });
    check("card_does_not_cover_the_word", covers === false);
    await esc(s);

    // Warm: first words and Stop.
    const v = await wordAt(s, "/^\\s*(control|models?|neural|policy|optimization|learning)\\s*$/i");
    const t1 = Date.now();
    await doubleClick(s, v.x, v.y);
    await s.until(`(document.querySelector(".explain-text")?.textContent ?? "").split(" ").length > 2 ? true : null`, 30000);
    const warmMs = Date.now() - t1;
    await s.js(`[...document.querySelectorAll(".explain button")].find((b) => b.textContent.trim() === "Stop")?.click()`);
    const stopped = await s.until(`/Stopped/.test(document.querySelector(".explain-status")?.textContent ?? "") ? true : null`, 2000);
    const a = await s.js(`document.querySelector(".explain-text").textContent`);
    await sleep(700);
    const b = await s.js(`document.querySelector(".explain-text").textContent`);
    check("stop_ends_the_answer", stopped === true && a === b, { warmMs });
    llmPid = Number(execSync(`powershell -NoProfile -c "(Get-CimInstance Win32_Process -Filter \\"Name='llama-server.exe' AND ParentProcessId=${s.app.pid}\\").ProcessId"`).toString().trim());
    const st = await s.call("inference.status", null);
    check("runtime_ready", st?.state === "ready", { status: st });
  } finally {
    s.app.kill();
  }
  // Asked of the process itself: listing processes can be slow while the GPU memory is given back.
  const alive = () => {
    try {
      return process.kill(llmPid, 0);
    } catch {
      return false;
    }
  };
  const ended = Date.now();
  while (alive() && Date.now() - ended < 3000) await sleep(50);
  check("model_process_ends_with_the_app", llmPid > 0 && !alive(), { endedMs: Date.now() - ended });
}

writeFileSync(join(out, "explain.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
