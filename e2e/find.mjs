// End-to-end check of asking the library and of the summaries (design language §17.11), with real
// mouse and keys. Words alone always; with a model when EP_LLM_DIR names a folder with one, and the
// founder's question on a real report when EP_FIND_PDF names it.
//
//   node e2e/find.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const llm = process.env.EP_LLM_DIR;
const real = process.env.EP_FIND_PDF;
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok: !!ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
const q = (sel) => `document.querySelector(${JSON.stringify(sel)})`;
const withText = (sel, text) => `[...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.textContent.trim().startsWith(${JSON.stringify(text)}))`;
const fixtures = ["product-manual", "report-business", "poster-research", "brochure"].map((n) => join(root, "fixtures/documents", `${n}.pdf`));
/** Files of summaries next to the library's papers. */
const kept = (data) => {
  const all = readdirSync(data, { recursive: true }).map(String);
  return all.filter((f) => /digests[\\/][^\\/]+\.json$/.test(f)).map((f) => JSON.parse(readFileSync(join(data, f), "utf8")));
};

async function drive(name, port, files, f) {
  const data = join(scratch, name);
  rmSync(data, { recursive: true, force: true });
  rmSync(`${data}-webview`, { recursive: true, force: true });
  const s = await launchApp({ exe, args: files, data, webview: `${data}-webview`, port, out, consoleLog: [] });
  const click = async (expr) => {
    const p = await s.js(`(() => { const e = ${expr}; if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
    if (!p) return false;
    for (const type of ["mousePressed", "mouseReleased"]) await s.send("Input.dispatchMouseEvent", { type, x: p.x, y: p.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
    await sleep(120);
    return true;
  };
  const key = async (k, code, vk, modifiers = 0) => {
    for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, modifiers });
    await sleep(80);
  };
  const askFor = async (text) => {
    await click(q(".ask-box textarea"));
    await key("a", "KeyA", 65, 2);
    await s.send("Input.insertText", { text });
    await key("Enter", "Enter", 13);
  };
  const count = (sel) => s.js(`document.querySelectorAll(${JSON.stringify(sel)}).length`);
  const done = (ms) => s.until(`document.querySelector(".ask-results .explain-status") ? true : null`, ms);
  const places = () => s.js(`[...document.querySelectorAll(".ask-near .ask-place")].map((e) => e.querySelector(".ask-place-head").innerText.replace(/\\s+/g, " "))`);
  try {
    await s.until(`document.querySelectorAll(".tab").length >= ${files.length} ? true : null`, 40000);
    await f({ s, data, click, key, askFor, count, done, places });
  } finally {
    s.app.kill();
  }
  await sleep(800);
}

// ---- words alone, without a model ------------------------------------------------------------------
delete process.env.EP_LLM_DIR;
await drive("find-words", 9404, fixtures, async ({ s, data, click, key, askFor, count, done }) => {
  // The key opens it, so does the library's list; the summaries are made by themselves.
  await key("L", "KeyL", 76, 10);
  const byKey = await s.until(`document.querySelector(".ask-box textarea") === document.activeElement ? true : null`, 4000);
  await click(withText(".library-nav .nav-item", "All papers"));
  const left = await count(".ask");
  await click(q(".library-nav .nav-ask"));
  check("asking_opens_by_key_and_from_the_library", byKey === true && left === 0 && (await count(".ask")) === 1, { byKey, left });
  await s.shot("find-empty.png");
  let files = [];
  for (let i = 0; i < 100 && files.length < fixtures.length; i++) {
    await sleep(200);
    files = kept(data);
  }
  check("quick_summaries_are_made_and_kept_with_the_papers", files.length === fixtures.length && files.every((d) => d.level === "basic" && d.sections.length > 0 && d.topics.length > 0), { files: files.map((d) => [d.title, d.sections.length, d.topics]) });

  const t0 = Date.now();
  await askFor("table with the motor power of the coffee grinder");
  const first = await s.until(`document.querySelector(".ask-place") ? true : null`, 10000);
  const ms = Date.now() - t0;
  await done(10000);
  const papers = await s.js(`[...document.querySelectorAll(".ask-paper .ask-title")].map((e) => e.textContent)`);
  const note = await s.js(`document.querySelector(".ask-results .explain-status")?.textContent`);
  const marked = await count(".ask-quote mark");
  await s.shot("find-words.png");
  check("words_find_at_once", first === true && ms < 2500 && papers?.length > 0 && marked > 0 && /without a language model/.test(note), { ms, papers, marked, note });

  // A place opens its paper, at its page, and flashes; coming back loses nothing.
  const target = await s.js(`(() => { const e = document.querySelector(".ask-paper .ask-place"); return { page: Number(e.querySelector(".ask-place-head .muted").textContent.replace(/\\D/g, "")), quote: e.querySelector(".ask-quote").textContent }; })()`);
  await click(q(".ask-paper .ask-place"));
  const flashed = await s.until(`document.querySelector(".pane.focused .page[data-page='${target.page}'] .flash-band") ? true : null`, 12000);
  const inView = await s.js(`(() => { const r = document.querySelector(".pane.focused .page[data-page='${target.page}']")?.getBoundingClientRect(); return !!r && r.bottom > 100 && r.top < innerHeight; })()`);
  check("a_place_opens_at_its_page", flashed === true && inView === true, { target, flashed, inView });
  await key("L", "KeyL", 76, 10);
  check("coming_back_keeps_what_was_found", (await s.until(`document.querySelectorAll(".ask-place").length > 0 ? true : null`, 3000)) === true && (await s.js(q(".ask-box textarea") + ".value")).includes("coffee grinder"));

  // The summary of a paper: its sections, each with its tags; without a model it says so.
  await click(withText(".ask-paper-head button", "Summary"));
  await s.until(`document.querySelectorAll(".summary-section").length > 0 ? true : null`, 4000);
  const sheet = await s.js(`({ sections: document.querySelectorAll(".summary-section").length, tags: document.querySelectorAll(".summary-section .tag").length, text: document.querySelector(".summary-sheet").innerText.slice(0, 300), model: [...document.querySelectorAll(".ep-dialog-actions button")].map((b) => b.textContent.trim()) })`);
  await s.shot("find-summary-basic.png");
  check("a_paper_has_a_summary_with_its_sections", sheet.sections > 2 && sheet.tags > 2 && /Only the headings/.test(sheet.text) && sheet.model.includes("Choose a language model"), sheet);
  await key("Escape", "Escape", 27);
  check("esc_closes", (await count(".summary-sheet")) === 0 && (await count(".ask-place")) > 0);

  // Nothing there: it says so.
  await askFor("xylophone zeppelin quokka");
  const nothing = await s.until(`/Nothing was found/.test(document.querySelector(".ask-results").textContent) ? true : null`, 8000);
  check("finding_nothing_says_so", nothing === true);

  // A section of the summary opens the paper there.
  await click(withText(".library-nav .nav-item", "All papers"));
  await click(q('.library-tools [aria-label="List"]'));
  await sleep(400);
  await click(q(".paper-item .row-action"));
  await click(withText(".ep-menu button", "Summary"));
  await s.until(`document.querySelectorAll(".summary-section").length > 0 ? true : null`, 4000);
  await click(`[...document.querySelectorAll(".summary-section .loc-chip")].at(-1)`);
  check("a_section_opens_the_paper", (await s.until(`document.querySelector(".pane.focused .flash-band") ? true : null`, 12000)) === true);
});

// ---- with a model ------------------------------------------------------------------------------------
if (llm) {
  process.env.EP_LLM_DIR = llm;
  const files = real ? [...fixtures, real] : fixtures;
  await drive("find-model", 9405, files, async ({ s, click, key, askFor, count, done, places }) => {
    await key("L", "KeyL", 76, 10);
    await s.until(`document.querySelector(".ask") ? true : null`, 4000);
    await s.until(`document.querySelector(".ask-summaries .spinner") ? null : true`, 60000);
    // The first search loads the model: what the words find is there long before.
    // A question that a short summary answers: the model sees the summaries alone when it chooses papers.
    const question = real ? "chart of carbon saving against NPV for the different technologies" : "cleaning the burrs of the coffee grinder";
    const t0 = Date.now();
    await askFor(question);
    await s.until(`document.querySelector(".ask-place") ? true : null`, 10000);
    const first = Date.now() - t0;
    const running = await count(".ask-status .spinner");
    await done(90000);
    const all = Date.now() - t0;
    const near = await places();
    const whys = await s.js(`[...document.querySelectorAll(".ask-paper > p.muted")].map((e) => e.textContent)`);
    const pictures = await s.until(`document.querySelectorAll(".ask-near img").length || null`, 8000);
    await s.shot("find-model.png");
    check("results_appear_at_once_and_grow", first < 2500 && running === 1 && all > first, { first, all });
    check("the_model_chooses_papers_and_nearest_places", near?.length > 0 && whys?.length > 0 && /chosen by/.test(await s.js(`document.querySelector(".ask-results .explain-status").textContent`)), { near, whys });
    if (real) {
      check("the_founders_question_finds_the_chart", near.slice(0, 3).some((x) => /^Figure (6|11)\b/.test(x)) && pictures > 0, { near, pictures });
      await askFor("نمودار سیو کربن و npv برای تکنولوژی های مختلف");
      await sleep(300);
      await done(60000);
      const persian = await places();
      await s.shot("find-model-2.png");
      check("a_question_in_another_language_finds_it_too", persian?.slice(0, 3).some((x) => /^Figure (6|11)\b/.test(x)), { persian });
    }
    // Stop ends it, and what was found stays.
    await askFor(real ? "energy and power" : "safety of the coffee grinder");
    await s.until(`document.querySelector(".ask-place") ? true : null`, 10000);
    await click(withText(".ask-box button", "Stop"));
    const stopped = await s.until(`/Stopped/.test(document.querySelector(".ask-status")?.textContent ?? "") ? true : null`, 3000);
    check("stop_ends_the_search", stopped === true && (await count(".ask-place")) > 0);

    // A written summary grows while it is read, and can be stopped.
    await click(withText(".library-nav .nav-item", "All papers"));
    await click(q('.library-tools [aria-label="List"]'));
    await sleep(400);
    await click(`[...document.querySelectorAll(".paper-item")].find((e) => /Aroma|manual|grinder/i.test(e.textContent))?.querySelector(".row-action") ?? document.querySelector(".paper-item .row-action")`);
    await click(withText(".ep-menu button", "Summary"));
    await s.until(`document.querySelectorAll(".summary-section").length > 0 ? true : null`, 4000);
    const t1 = Date.now();
    await click(withText(".ep-dialog-actions button", "Make the summary"));
    const grown = await s.until(`document.querySelectorAll(".summary-section > p:not(.summary-tags)").length || null`, 90000);
    const early = await s.js(`!!document.querySelector(".summary-sheet .spinner")`);
    const at = Date.now() - t1;
    await s.shot("find-summary-growing.png");
    const ready = await s.until(`document.querySelector(".summary-sheet .spinner") ? null : true`, 300000);
    const end = Date.now() - t1;
    const written = await s.js(`({ text: document.querySelector(".summary-sheet").innerText.slice(0, 1500), sections: document.querySelectorAll(".summary-section > p:not(.summary-tags)").length, whole: !!document.querySelector(".summary-sheet > p:not(.muted, .summary-tags)") && !!document.querySelector(".summary-tags strong") })`);
    await s.shot("find-summary.png");
    check("a_summary_grows_while_it_is_made", grown > 0 && early === true && ready === true && written.sections >= grown && written.whole === true, { at, end, written });
    await click(withText(".ep-dialog-actions button", "Make it again"));
    await s.until(`document.querySelector(".summary-sheet .spinner") ? true : null`, 4000);
    await click(withText(".ep-dialog-actions button", "Stop"));
    const gone = await s.until(`document.querySelector(".summary-sheet .spinner") ? null : true`, 15000);
    check("making_summaries_can_be_stopped", gone === true);
  });
}
writeFileSync(join(out, "find.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
