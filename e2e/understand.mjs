// End-to-end check of understanding papers by themselves (design language §17.12): nothing is asked for,
// the brain mark, closing too early and going on, people first. The part with a model runs when
// EP_LLM_DIR names a folder with one; EP_REAL_DATA (a copy of a real library's data folder) adds the
// founder's case: sorting all its papers.
//
//   node e2e/understand.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const llm = process.env.EP_LLM_DIR;
const real = process.env.EP_REAL_DATA;
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok: !!ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
const docs = ["product-manual", "report-business"].map((n) => join(root, "fixtures/documents", `${n}.pdf`));
const q = (sel) => `document.querySelector(${JSON.stringify(sel)})`;
const withText = (sel, text) => `[...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.textContent.trim().startsWith(${JSON.stringify(text)}))`;

const FRAMES = `new Promise((done) => { const el = document.querySelector(".pane.focused .scroller"); const gaps = []; let last = performance.now(); const t0 = last; const step = (t) => { gaps.push(t - last); last = t; el.scrollTop += 6; if (t - t0 < 2500) requestAnimationFrame(step); else done({ n: gaps.length, worst: Math.round(Math.max(...gaps.slice(2))), slow: gaps.filter((g) => g > 50).length }); }; requestAnimationFrame(step); })`;
/** Scrolling on this machine, now, with nothing in the background. */
let alone = { n: 0, worst: 0, slow: 0 };

async function drive(data, port, files, f) {
  const s = await launchApp({ exe, args: files, data, webview: `${data}-webview`, port, out, consoleLog: [] });
  const click = async (expr) => {
    const p = await s.js(`(() => { const e = ${expr}; if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
    if (!p) return false;
    for (const type of ["mousePressed", "mouseReleased"]) await s.send("Input.dispatchMouseEvent", { type, x: p.x, y: p.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
    await sleep(150);
    return true;
  };
  const key = async (k, code, vk, modifiers = 0) => {
    for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, modifiers });
    await sleep(80);
  };
  const library = async (view = "List") => {
    if (!(await s.js(`!!document.querySelector(".library-view")`))) await s.js(`document.querySelector('.topbar [aria-label^="Library"]').click()`);
    await s.until(`document.querySelector(".library-view")`, 8000);
    await click(withText(".library-nav .nav-item", "All papers"));
    await click(q(`.library-tools [aria-label="${view}"]`));
    await sleep(500);
  };
  /** The laptop may be low on power while this runs: the person says "go on anyway". */
  const anyway = async () => {
    await s.until(`document.querySelector(".understanding") ? true : null`, 60000);
    await sleep(1500);
    if (!/battery/.test((await s.js(`document.querySelector(".understanding")?.textContent`)) ?? "")) return false;
    await click(q(".understanding"));
    await s.until(`document.querySelector(".ep-menu")`, 3000);
    await s.shot("understand-battery.png");
    return click(withText(".ep-menu button", "Go on anyway"));
  };
  const marks = () => s.js(`({ ready: document.querySelectorAll(".brain-mark:not(.working)").length, working: document.querySelectorAll(".brain-mark.working").length, says: [...document.querySelectorAll(".brain-mark")].map((e) => e.title) })`);
  try {
    await s.until(`document.querySelector(".topbar")`, 20000);
    await f({ s, click, key, library, marks, anyway });
  } finally {
    s.app.kill();
  }
  await sleep(1500);
}
const fresh = (name) => {
  const data = join(scratch, name);
  for (const d of [data, `${data}-webview`]) rmSync(d, { recursive: true, force: true });
  return data;
};

// ---- without a model: quick digests, no marks, no noise ---------------------------------------------
delete process.env.EP_LLM_DIR;
await drive(fresh("understand-none"), 9412, docs, async ({ s, click, library, marks }) => {
  await library();
  let infos = [];
  for (let i = 0; i < 60; i++) {
    infos = (await s.call("knowledge.digests", {})) ?? [];
    if (infos.length === docs.length && infos.every((d) => d.level)) break;
    await sleep(500);
  }
  await sleep(800);
  const m = await marks();
  const errors = await s.js(`document.querySelectorAll(".ep-banner").length`);
  check("without_a_model_nothing_is_marked_and_nothing_is_reported", infos.every((d) => d.level === "basic" && !d.ready) && m.ready + m.working === 0 && errors === 0, { infos: infos.map((d) => [d.level, d.ready, d.problem]), m, errors });
  await click(q(".paper-open"));
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 15000);
  await sleep(2500);
  alone = await s.js(FRAMES);
  for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: ",", code: "Comma", windowsVirtualKeyCode: 188, modifiers: 2 });
  await s.until(`document.querySelector(".ep-dialog .settings")`, 4000);
  check("settings_say_when_no_model_is_there", await s.until(`/No model that writes answers/.test(document.querySelector(".llm")?.innerText ?? "") ? true : null`, 15000));
  await click(q(".ep-dialog-actions button"));
});

// ---- with a model -------------------------------------------------------------------------------------
if (llm) {
  process.env.EP_LLM_DIR = llm;
  const data = fresh("understand");
  // Opened quickly and closed again: the window is ended in the middle of the work.
  await drive(data, 9413, docs, async ({ s, library, marks, anyway }) => {
    await s.until(`document.querySelectorAll(".tab").length >= ${docs.length} ? true : null`, 30000);
    const ready = await s.until(`document.querySelector(".understanding") ? true : null`, 60000);
    const low = await anyway();
    await s.shot("understand-reading.png");
    await library();
    const working = await s.until(`document.querySelector(".brain-mark.working") ? true : null`, 20000);
    const m = await marks();
    await s.shot("understand-working.png");
    check("papers_are_understood_by_themselves", ready === true && working === true, { m, low });
    // At least one section, then closed.
    for (let i = 0; i < 120; i++) {
      const d = (await s.call("knowledge.digests", {})) ?? [];
      if (d.some((x) => x.level === "full")) break;
      await sleep(500);
    }
    check("it_is_not_finished_yet", ((await s.call("knowledge.digests", {})) ?? []).some((d) => !d.ready));
  });
  await drive(data, 9414, [], async ({ s, click, key, library, marks, anyway }) => {
    const load = await s.until(`document.querySelector(".topbar") ? true : null`, 10000);
    const t0 = Date.now();
    let st = null;
    for (let i = 0; i < 200; i++) {
      st = await s.call("inference.status", null);
      if (st?.state === "ready") break;
      await sleep(250);
    }
    check("the_model_is_made_ready_by_itself", load === true && st?.state === "ready", { ms: Date.now() - t0, model: st?.model });
    await anyway();
    await library();
    await s.until(`document.querySelector(".brain-mark.working") ? true : null`, 60000);
    // What a person asks goes first, though the model is at work for the background.
    const t1 = Date.now();
    const found = await s.call("knowledge.find", { question: "motor power of the coffee grinder" });
    const asked = Date.now() - t1;
    check("a_person_comes_first", found?.used_model === true && asked < 9000, { asked, model: found?.model, related: found?.related?.length, error: found?.error });
    // Reading stays fluid: frames while a paper is scrolled.
    await click(q(".paper-open"));
    await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 15000);
    await sleep(2500);
    const frames = await s.js(FRAMES);
    check("reading_stays_fluid", frames.n > 60 && frames.slow <= alone.slow + 4 && frames.n > alone.n * 0.7, { alone, frames });
    await library("Covers");
    const all = await s.until(`document.querySelectorAll(".brain-mark:not(.working)").length === ${docs.length} ? true : null`, 420000);
    const m = await marks();
    await s.shot("understand-ready.png");
    check("what_was_closed_too_early_is_gone_on_with", all === true && m.says.every((x) => x === "Understood by AI, ready"), m);
    const after = await s.until(`document.querySelector(".understanding") ? null : true`, 10000);
    const state = await s.call("knowledge.digest_status", null);
    check("when_all_is_done_it_is_quiet", after === true && state?.running === false, state);
    // The mark opens the summary.
    await click(q(".brain-mark"));
    check("the_mark_opens_the_summary", (await s.until(`document.querySelectorAll(".summary-section").length > 0 ? true : null`, 4000)) === true);
    await key("Escape", "Escape", 27);
    // Still loaded, so it answers at once.
    const t2 = Date.now();
    const again = await s.call("knowledge.find", { question: "price of flour" });
    check("the_model_stays_ready", again?.used_model === true && Date.now() - t2 < 9000 && (await s.call("inference.status", null))?.state === "ready", { ms: Date.now() - t2, model: again?.model });
  });

  if (real) {
    // The founder's library: all papers at once.
    await drive(real, 9415, [], async ({ s, click, key, library }) => {
      await library("Covers");
      const n = await s.until(`document.querySelectorAll(".paper-cover").length || null`, 10000);
      await sleep(1500);
      await key("a", "KeyA", 65, 2);
      await click(withText(".library-selection button", "Suggest folders"));
      const t0 = Date.now();
      const done = await s.until(`document.querySelector(".sort-group") ? "ready" : document.querySelector(".sort-sheet .ep-button") ? "failed" : null`, 240000);
      const groups = await s.js(`[...document.querySelectorAll(".sort-group")].map((g) => ({ name: g.querySelector("input.sort-name")?.value ?? g.querySelector(".sort-name")?.textContent, n: g.querySelectorAll(".sort-title").length }))`);
      await s.shot("understand-sorted.png");
      check("all_papers_of_a_library_are_sorted", done === "ready" && groups.length >= 3 && groups.reduce((a, g) => a + g.n, 0) === n && groups.filter((g) => !/Not sure/.test(g.name)).reduce((a, g) => a + g.n, 0) > n / 2, { done, ms: Date.now() - t0, n, groups });
      await key("Escape", "Escape", 27);
    });
  }
}
writeFileSync(join(out, "understand.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
