// End-to-end check of the right-click menu and of Explain for figures, tables and equations
// (P1.M5.UI.2, UI.3, UND.3–UND.6, UND.8; design language §17.13), with real mouse input.
//
//   node e2e/objects.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Without the layout model: the menu is about the place, without AI rows.
// With it (EP_MODELS_DIR): the menu names the figure, table, equation or paragraph under the pointer;
// its copy rows work; a selection has its own menu; no AI rows, as there is no language model.
// With a language model (EP_LLM_DIR): Explain answers in the card, simpler and deeper in the same
// card; a question to a table marks the cells; Ctrl+E explains the selection.
// With a vision model (EP_VLM_DIR): the picture is looked at only after a yes, asked once a paper.

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const paper = join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf");
const equations = join(root, ".fixtures/papers/arxiv-2310.09367v1.pdf");
const { EP_MODELS_DIR: models, EP_LLM_DIR: llm, EP_VLM_DIR: vlm } = process.env;
const report = {};
const failures = [];
const consoleLog = [];
const check = (name, ok, detail) => {
  report[name] = { ok: !!ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
async function run(tag, { file = paper, modelsDir, llmDir, port }, body) {
  const data = join(scratch, `objects-${tag}`);
  const webview = join(scratch, `objects-${tag}-webview`);
  rmSync(data, { recursive: true, force: true });
  rmSync(webview, { recursive: true, force: true });
  process.env.EP_MODELS_DIR = modelsDir;
  if (llmDir) process.env.EP_LLM_DIR = llmDir;
  else delete process.env.EP_LLM_DIR;
  const s = await launchApp({ exe, args: [file], data, webview, port, out, consoleLog });
  try {
    await s.until(`document.querySelectorAll(".pane.focused .text-layer span[data-page]").length > 50 ? true : null`, 30000);
    const papers = await s.call("workspace.list_papers", {});
    const h = helpers(s, papers[0]);
    await body(s, h);
  } catch (e) {
    check(`${tag}_runs`, false, { error: String(e?.stack ?? e) });
  } finally {
    await s.shot(`objects-${tag}-last.png`);
    s.ws.close();
    s.app.kill();
    await sleep(1000);
  }
}

function helpers(s, p) {
  const file = p.file;
  const mouse = (type, x, y, button = "left", n = 1) => s.send("Input.dispatchMouseEvent", { type, x, y, button, clickCount: n });
  const press = async (x, y, button = "left", n = 1) => {
    await mouse("mousePressed", x, y, button, n);
    await mouse("mouseReleased", x, y, button, n);
  };
  const key = async (k, code, vk, modifiers = 0) => {
    const text = k === "Enter" ? { text: "\r" } : {};
    for (const type of [k === "Enter" ? "keyDown" : "rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, modifiers, ...(type === "keyUp" ? {} : text) });
  };
  const esc = () => key("Escape", "Escape", 27);
  const info = s.call("document.info", { file });
  /** Put a place of a page in the middle of the view; where it is on the screen. */
  const point = async (rect, fx = 0.5, fy = 0.5) => {
    await s.js(`(() => { const b = document.querySelector(".page-box"); b.value = "${rect.page}"; b.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); })()`);
    const page = `document.querySelector(".pane.focused .page[data-page='${rect.page}']")`;
    await s.until(`${page} ? true : null`);
    await sleep(700);
    const w = (await info).sizes[rect.page - 1].width;
    const where = `(() => { const r = ${page}.getBoundingClientRect(); const k = r.width / ${w};
      return { x: r.left + ${rect.x0 + (rect.x1 - rect.x0) * fx} * k, y: r.top + ${rect.y0 + (rect.y1 - rect.y0) * fy} * k }; })()`;
    const at = await s.js(where);
    await s.js(`(() => { const e = document.querySelector(".pane.focused .scroller"); e.scrollTop += ${at.y} - e.getBoundingClientRect().top - e.clientHeight / 2; })()`);
    await sleep(500);
    return s.js(where);
  };
  /** Right-click; the rows of the menu. */
  const menu = async (at) => {
    await press(at.x, at.y, "right");
    return s.until(`(() => { const m = document.querySelector(".ep-menu"); return m ? { name: m.getAttribute("aria-label"), rows: [...m.querySelectorAll(".ep-menu-label")].map((e) => e.textContent) } : null; })()`, 4000);
  };
  const choose = (label) => s.js(`(() => { const b = [...document.querySelectorAll(".ep-menu button")].find((x) => x.querySelector(".ep-menu-label").textContent === ${JSON.stringify(label)}); b?.click(); return !!b; })()`);
  const closed = (sel) => s.until(`document.querySelector(${JSON.stringify(sel)}) ? null : true`, 3000);
  const toast = () => s.until(`document.querySelector(".ep-toast span")?.textContent ?? null`, 8000);
  const objects = () =>
    s.until(
      `window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.preparation", input: ${JSON.stringify({ file })}, deadlineMs: null, approved: [] })
        .then((r) => r.stages.find((x) => x.stage === "objects").status === "done" ? true : null)`,
      180000,
    );
  const paragraph = async (page) => {
    const os = (await s.call("document.objects", { file, page })).objects;
    return os.filter((o) => o.kind === "text").sort((a, b) => (b.rect.x1 - b.rect.x0) * (b.rect.y1 - b.rect.y0) - (a.rect.x1 - a.rect.x0) * (a.rect.y1 - a.rect.y0))[0];
  };
  /** Wait for the answer of the card. */
  const answered = (ms = 120000) => s.until(`(() => { const e = [...document.querySelectorAll(".explain-status")].find((x) => /Checked|Corrected|Not checked/.test(x.textContent)); return e && !document.querySelector(".explain .spinner") ? e.textContent : null; })()`, ms);
  const card = () =>
    s.js(`(() => { const c = document.querySelector(".explain"); if (!c) return null; const r = c.getBoundingClientRect();
      return { title: c.querySelector(".ep-card-head strong")?.textContent, text: c.querySelector(".explain-text")?.textContent ?? "", status: [...c.querySelectorAll(".explain-status")].map((e) => e.textContent),
        symbols: [...c.querySelectorAll(".explain-symbols tbody tr")].map((r) => [...r.children].map((e) => e.textContent)), buttons: [...c.querySelectorAll(".explain-actions button")].map((b) => b.textContent.trim() || b.getAttribute("aria-label")),
        inside: r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight }; })()`);
  const cardButton = (label) => s.js(`(() => { const b = [...document.querySelectorAll(".explain button")].find((x) => x.textContent.trim() === ${JSON.stringify(label)}); b?.click(); return !!b; })()`);
  /** At the smallest window: what leaves the window or its box. */
  const fits = async (sel) => {
    await s.send("Emulation.setDeviceMetricsOverride", { width: 800, height: 500, deviceScaleFactor: 1, mobile: false });
    await sleep(500);
    const wrong = await s.js(`(() => { const box = document.querySelector(${JSON.stringify(sel)}); if (!box) return ["not there"]; const b = box.getBoundingClientRect(); const w = [];
      if (b.left < 0 || b.top < 0 || b.right > innerWidth || b.bottom > innerHeight) w.push("the box " + [b.left, b.top, b.right, b.bottom].map(Math.round));
      for (const e of box.querySelectorAll("button, input, p, strong")) { if (e.closest(".table-scroll")) continue; const r = e.getBoundingClientRect(); if (r.width > 0 && (r.right > b.right + 1 || r.left < b.left - 1 || (e.tagName === "BUTTON" && e.scrollWidth > e.clientWidth + 2))) w.push(e.textContent.trim().slice(0, 30) || e.getAttribute("aria-label")); }
      return w; })()`);
    await s.shot(`objects-small-${sel.replace(/\W/g, "")}.png`);
    await s.send("Emulation.clearDeviceMetricsOverride");
    await sleep(300);
    check(`fits_the_smallest_window_${sel.slice(1)}`, wrong.length === 0, { wrong });
  };
  return { fits, file, paper: p.id, press, mouse, key, esc, point, menu, choose, closed, toast, objects, paragraph, answered, card, cardButton };
}

const rowsOk = (m, rows) => !!m && m.rows.length === rows.length && rows.every((r, i) => m.rows[i] === r);
/** A word on the screen that contains `re`, with room around it. */
const wordAt = (s, re) =>
  s.until(`(() => {
    for (const e of document.querySelectorAll(".pane.focused .text-layer span[data-page]")) {
      if (!${re}.test(e.textContent)) continue;
      const r = e.getBoundingClientRect();
      if (r.width < 20 || r.top < 120 || r.bottom > innerHeight - 320) continue;
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, left: r.left, right: r.right, text: e.textContent };
    }
    return null;
  })()`, 8000);
const WORD = "/^\\s*(learning|reinforcement|control|optimization|models?|policy)\\s*$/i";

// ---- no models at all -------------------------------------------------------------------------
const empty = join(scratch, "objects-no-models");
mkdirSync(empty, { recursive: true });
await run("none", { modelsDir: empty, port: 9361 }, async (s, h) => {
  const w = await wordAt(s, WORD);
  const m = await h.menu(w);
  await s.shot("objects-menu-place.png");
  check("a_place_has_its_menu_without_ai", rowsOk(m, ["Copy text", "Copy page as image"]), { menu: m });
  await h.esc();
  check("esc_closes_the_menu", (await h.closed(".ep-menu")) === true);
  // The other mouse button marks nothing.
  const marked = await s.js(`(() => { const g = getSelection(); return !g.isCollapsed || !!document.querySelector(".selection-bar"); })()`);
  check("right_click_selects_nothing", marked === false);
  // A selection has its own menu.
  await h.press(w.x, w.y, "left", 1);
  await h.press(w.x, w.y, "left", 2);
  await sleep(400);
  const sel = await h.menu(w);
  await s.shot("objects-menu-selection.png");
  check("a_selection_has_its_own_menu", sel && sel.rows.includes("Copy") && sel.rows.includes("Highlight") && !sel.rows.some((r) => /Explain/.test(r)) && sel.rows.length <= 9, { menu: sel });
  const kept = await s.js(`getSelection().toString().trim()`);
  check("the_menu_keeps_the_selection", w.text.includes(kept) && kept.length > 2, { kept, word: w.text });
  await h.choose("Highlight");
  const marks = await s.until(
    `window.__TAURI_INTERNALS__.invoke("bus_call", { command: "document.annotations", input: ${JSON.stringify({ paper: h.paper, file: h.file })}, deadlineMs: null, approved: [] })
      .then((a) => (a.annotations.length > 0 ? a.annotations.map((x) => x.kind?.kind ?? x.kind ?? x.type) : null))`,
    5000,
  );
  check("highlight_from_the_menu", marks?.length === 1, { marks });
});

// ---- the layout model, no language model -------------------------------------------------------
if (models) {
  await run("layout", { modelsDir: models, port: 9362 }, async (s, h) => {
    await h.objects();
    const c = await s.call("document.contents", { file: h.file });
    const copy = async (row, says) => {
      const there = await h.choose(row);
      const said = await h.toast();
      const ok = there && says.test(said ?? "");
      await s.until(`document.querySelector(".ep-toast") ? null : true`, 7000);
      return { row, there, said, ok };
    };
    const seen = {};
    const copies = [];
    for (const [kind, o] of [["figure", c.figures[0]], ["table", c.tables[0]], ["equation", c.equations[0]], ["paragraph", await h.paragraph(2)]]) {
      if (!o) continue;
      const at = await h.point(o.rect);
      const t0 = Date.now();
      seen[kind] = { ...(await h.menu(at)), ms: Date.now() - t0, label: o.label };
      await s.shot(`objects-menu-${kind}.png`);
      const rows = { figure: ["Copy image", "Copy caption"], table: ["Copy CSV"], equation: ["Copy text"], paragraph: ["Copy text"] }[kind];
      for (const [n, row] of rows.entries()) {
        if (n > 0) await h.menu(at);
        copies.push(await copy(row, /copied/i));
      }
      if (rows.length === 0) await h.esc();
      await h.closed(".ep-menu");
    }
    check("the_menu_is_about_what_is_under_the_pointer", seen.figure.name === seen.figure.label && seen.table.name === seen.table.label && seen.equation.name === `Equation ${seen.equation.label}`.trim() && seen.paragraph.name === "This paragraph", { seen });
    check("no_ai_rows_without_a_model", Object.values(seen).every((m) => m.rows.every((r) => !/Explain|Ask|Look/.test(r))));
    check("short_menus_at_once", Object.values(seen).every((m) => m.rows.length <= 9 && m.ms < 1000), {});
    check("copy_rows_work", copies.length >= 5 && copies.every((x) => x.ok), { copies });
    // A caption stands for its figure.
    const cap = c.figures.find((f) => f.caption_rect);
    if (cap) {
      const m = await h.menu(await h.point(cap.caption_rect));
      check("a_caption_stands_for_its_figure", m?.name === cap.label, { menu: m, label: cap.label });
      await h.esc();
    }
    // Open as table from the menu: no question box without a language model.
    await h.menu(await h.point(c.tables[0].rect));
    const opened = await h.choose("Open as table");
    const cells = await s.until(`document.querySelectorAll(".table-view tbody td").length || null`, 10000);
    check("table_opens_from_the_menu", opened && cells > 4 && !(await s.js(`!!document.querySelector(".table-ask")`)), { cells });
    await h.esc();
    await h.closed(".table-view");
    // To notes.
    await h.menu(await h.point((await h.paragraph(2)).rect));
    await h.choose("Add to notes");
    const noted = await s.until(`document.querySelector(".side.right, aside[aria-label*='otes']")?.textContent.length > 60 ? true : null`, 6000);
    check("a_paragraph_goes_to_the_notes", noted === true);
  });

  if (llm) {
    await run("explain", { modelsDir: models, llmDir: llm, port: 9363 }, async (s, h) => {
      if (vlm) await s.call("inference.add_folder", { path: vlm });
      await h.objects();
      const c = await s.call("document.contents", { file: h.file });
      const ask = async (o, row) => {
        const at = await h.point(o.rect);
        const m = await h.menu(at);
        const t0 = Date.now();
        const there = await h.choose(row);
        const shown = await s.until(`document.querySelector(".explain") ? true : null`, 2000);
        const shownMs = Date.now() - t0;
        await s.until(`(document.querySelector(".explain-text")?.textContent ?? "").length > 5 ? true : null`, 120000);
        const firstMs = Date.now() - t0;
        const status = await h.answered();
        return { menu: m, there, shown, shownMs, firstMs, doneMs: Date.now() - t0, status, card: await h.card(), at };
      };
      const fig = await ask(c.figures[0], `Explain ${c.figures[0].label}`);
      await s.shot("objects-explain-figure.png");
      check("explain_a_figure", fig.there && fig.shown && fig.card?.title === c.figures[0].label && fig.card.text.length > 40 && fig.card.status.some((x) => /not looked at/.test(x)) && fig.card.inside, fig);

      await h.fits(".explain .ep-card");
      // Simpler, then deeper: the same card, another text.
      await h.cardButton("Simpler");
      await s.until(`document.querySelector(".explain .spinner") ? true : null`, 5000);
      await h.answered();
      const simple = await h.card();
      await h.cardButton("Deeper");
      await s.until(`document.querySelector(".explain .spinner") ? true : null`, 5000);
      await h.answered();
      const deep = await h.card();
      await s.shot("objects-explain-deeper.png");
      check("simpler_and_deeper_in_the_same_card", simple?.title === fig.card.title && deep?.title === fig.card.title && simple.text !== fig.card.text && deep.text !== simple.text && deep.text.length > simple.text.length, { normal: fig.card.text, simple: simple?.text, deep: deep?.text });
      // Asked again: kept, at once.
      const t1 = Date.now();
      await h.cardButton("Normal");
      await h.answered(60000);
      report.normal_again_ms = Date.now() - t1;

      if (vlm) {
        check("look_is_offered", deep.buttons.includes("Look at the picture"), { buttons: deep.buttons });
        await h.cardButton("Look at the picture");
        const asked = await s.until(`document.querySelector(".ep-dialog")?.textContent ?? null`, 4000);
        await sleep(400);
        await s.shot("objects-look-consent.png");
        await h.fits(".ep-dialog");
        await h.esc();
        await h.closed(".ep-dialog");
        await sleep(1500);
        const none = await s.js(`!document.querySelector(".explain-seen") && !!document.querySelector(".explain")`);
        check("looking_needs_a_yes", /on this computer.*Not now/.test(asked ?? "") && none === true, { asked });
        await h.cardButton("Look at the picture");
        await s.until(`document.querySelector(".ep-dialog") ? true : null`, 4000);
        await s.js(`document.querySelector(".ep-dialog-actions .ep-button:last-child, .ep-dialog-actions button:last-child").click()`);
        const t0 = Date.now();
        const seen = await s.until(`(() => { const e = document.querySelector(".explain-seen"); return e && !e.querySelector(".spinner") ? e.textContent : null; })()`, 300000);
        await s.shot("objects-looked.png");
        const said = await h.card();
        check("the_picture_is_looked_at", (seen ?? "").length > 60, { seen, ms: Date.now() - t0, card: said });
        // Asked once a paper.
        await h.cardButton("Look again");
        await sleep(800);
        check("asked_once_a_paper", (await s.js(`!document.querySelector(".ep-dialog")`)) === true);
        await h.cardButton("Stop");
      }
      await h.esc();
      await h.closed(".explain");

      const par = await ask(await h.paragraph(2), "Explain this paragraph");
      await s.shot("objects-explain-paragraph.png");
      check("explain_a_paragraph", par.there && par.card?.text.length > 40 && par.card.inside, par);
      await h.esc();

      // Ask the table.
      const tab = c.tables.find((o) => o.label === "Table 2") ?? c.tables[0];
      const m = await h.menu(await h.point(tab.rect));
      await h.choose(`Ask ${tab.label}`);
      const focused = await s.until(`document.activeElement?.closest(".table-ask") ? true : null`, 8000);
      await s.until(`document.querySelectorAll(".table-view tbody td").length || null`, 10000);
      await s.send("Input.insertText", { text: "Which model has the lowest error for Tm?" });
      await h.key("Enter", "Enter", 13);
      const t2 = Date.now();
      const said = await s.until(`document.querySelector(".table-answer")?.textContent ?? null`, 90000);
      const cells = await s.js(`[...document.querySelectorAll("[data-role]")].map((e) => [e.dataset.cell, e.dataset.role, e.textContent])`);
      await s.shot("objects-table-answer.png");
      check("a_table_answers_from_its_cells", /PhysReg MLP/.test(said ?? "") && cells.some((x) => x[1] === "answer") && focused === true, { rows: m?.rows, said, cells, ms: Date.now() - t2 });
      await h.fits(".table-view");
      // Sorting keeps the marks on their cells.
      await s.js(`document.querySelector(".table-view th .sort").click()`);
      const sorted = await s.js(`[...document.querySelectorAll("[data-role]")].map((e) => [e.dataset.cell, e.dataset.role, e.textContent])`);
      check("sorting_keeps_the_marks", JSON.stringify([...sorted].sort()) === JSON.stringify([...cells].sort()), { sorted });
      await s.js(`(() => { const i = document.querySelector(".table-ask input"); i.focus(); i.select(); })()`);
      await s.send("Input.insertText", { text: "What was the weather in Paris?" });
      await h.key("Enter", "Enter", 13);
      await s.until(`document.querySelector(".table-said .spinner") ? true : null`, 3000);
      const not = await s.until(`(() => { const e = document.querySelector(".table-said"); return e && !e.querySelector(".spinner") && e.textContent ? e.textContent : null; })()`, 90000);
      check("a_table_says_what_it_does_not_hold", /does not answer|could not/i.test(not ?? ""), { not });
      await h.esc();
      await h.closed(".table-view");

      // Ctrl+E explains the selection.
      const w = await wordAt(s, WORD);
      const word = w ?? (await h.point({ page: 1, x0: 0, x1: 10, y0: 300, y1: 310 }), await wordAt(s, WORD));
      await h.press(word.x, word.y, "left", 1);
      await h.press(word.x, word.y, "left", 2);
      await s.until(`document.querySelector(".explain") ? true : null`, 3000);
      await h.esc();
      await h.closed(".explain");
      await h.key("e", "KeyE", 69, 2);
      const byKey = await s.until(`document.querySelector(".explain .ep-card-head strong")?.textContent ?? null`, 4000);
      check("ctrl_e_explains_the_selection", !!byKey && word.text.includes(byKey), { byKey, word: word.text });
      await h.esc();
      // In both themes.
      for (const theme of ["Dark", "Light"]) {
        await h.esc();
        await h.closed(".explain");
        await s.js(`document.querySelector('.topbar [aria-label^="Theme"]').click()`);
        await s.until(`document.querySelector(".ep-menu") ? true : null`);
        await s.js(`[...document.querySelectorAll(".ep-menu button")].find((b) => /${theme}/.test(b.textContent))?.click()`);
        await h.menu(await h.point(tab.rect));
        await sleep(400);
        await s.shot(`objects-menu-${theme.toLowerCase()}.png`);
        await h.choose(`Explain ${tab.label}`);
        await h.answered();
        await s.shot(`objects-explain-table-${theme.toLowerCase()}.png`);
      }
      const table = await h.card();
      check("explain_a_table", table?.text.length > 40 && table.inside, { card: table });
    });

    await run("equation", { file: equations, modelsDir: models, llmDir: llm, port: 9364 }, async (s, h) => {
      await h.objects();
      const c = await s.call("document.contents", { file: h.file });
      const eq = c.equations.find((e) => e.label === "(2)") ?? c.equations[0];
      const m = await h.menu(await h.point(eq.rect));
      await h.choose(m.rows.find((r) => /^Explain/.test(r)));
      // What code knows comes first.
      const symbols = await s.until(`document.querySelectorAll(".explain-symbols tbody tr").length || null`, 20000);
      await h.answered();
      const card = await h.card();
      await s.shot("objects-explain-equation.png");
      check("explain_an_equation_with_its_symbols", symbols > 0 && card.text.length > 40 && card.inside && card.symbols.every((r) => r[0] && r[1]) && !card.symbols.some((r) => /^(it|is|we)$/.test(r[0])), { menu: m, card });
    });
  }
}

report.console = consoleLog;
check("no_errors_in_the_console", consoleLog.length === 0, { consoleLog });
writeFileSync(join(out, "objects.json"), JSON.stringify(report, null, 2));
for (const [n, r] of Object.entries(report)) if (r && typeof r === "object" && "ok" in r) console.log(`${r.ok ? "ok  " : "FAIL"} ${n}${r.ok ? "" : ` ${JSON.stringify(r)}`}`);
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
