// End-to-end check of library folders and several papers at once (design language §17.10), with real
// mouse and keys. Suggested sorting runs with a model when EP_LLM_DIR names a folder with one.
//
//   node e2e/folders.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const llm = process.env.EP_LLM_DIR;
const docs = ["product-manual", "poster-event", "poster-research", "report-business", "slides", "brochure", "form-irs-w4-2024"].map((n) => join(root, "fixtures/documents", `${n}.pdf`));
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok: !!ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
const data = join(scratch, "folders");
rmSync(data, { recursive: true, force: true });
const start = (args) => launchApp({ exe, args, data, webview: join(scratch, "folders-webview"), port: 9402, out, consoleLog: [] });
let s = await start(docs);
const mouse = (type, x, y, extra = {}) => s.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, ...extra });
const at = (expr) => s.js(`(() => { const e = ${expr}; if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
const click = async (expr, modifiers = 0) => {
  const p = await at(expr);
  if (!p) return false;
  await s.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y, modifiers });
  await mouse("mousePressed", p.x, p.y, { modifiers });
  await mouse("mouseReleased", p.x, p.y, { modifiers });
  await sleep(150);
  return true;
};
const key = async (k, code, vk, modifiers = 0) => {
  for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, modifiers });
  await sleep(120);
};
const type = async (text) => {
  await s.send("Input.insertText", { text });
  await key("Enter", "Enter", 13);
  await sleep(350);
};
const q = (sel) => `document.querySelector(${JSON.stringify(sel)})`;
const withText = (sel, text) => `[...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.textContent.trim().startsWith(${JSON.stringify(text)}))`;
const cover = (i) => `document.querySelectorAll(".paper-cover")[${i}]`;
const count = (sel) => s.js(`document.querySelectorAll(${JSON.stringify(sel)}).length`);
const undo = async () => {
  await s.until(`${withText("button", "Undo")} ? true : null`, 4000);
  await click(withText("button", "Undo"));
  await sleep(600);
};
const tree = () => s.js(`[...document.querySelectorAll(".folder-tree .nav-item")].map((e) => e.textContent.trim().replace(/\\s+/g, " "))`);
const library = async () => {
  if (!(await s.js(`!!document.querySelector(".library-view")`))) await s.js(`document.querySelector('.topbar [aria-label^="Library"]').click()`);
  await s.until(`document.querySelector(".library-view")`, 8000);
  await click(q('.library-tools [aria-label="Covers"]'));
  await sleep(700);
};
const n = docs.length;

try {
  await s.until(`document.querySelectorAll(".tab").length >= ${n} ? true : null`, 40000);
  await library();
  await s.until(`document.querySelectorAll(".paper-cover").length === ${n} ? true : null`, 8000);

  // Folders, and a folder inside a folder.
  await click(withText(".library-nav button", "New folder"));
  await type("Work");
  await click(withText(".library-nav button", "New folder"));
  await type("Home papers");
  await click(q('.library-nav [aria-label="Folder Work"]'));
  await click(withText(".ep-menu button", "New folder inside"));
  await type("Reports");
  check("folders_inside_folders", (await count(".folder-tree .folder-tree .nav-item")) === 1 && (await count(".folder-tiles .folder-tile")) === 2, { tree: await tree() });

  // Selecting: the mark, Ctrl, Shift, all, none.
  await click(`${cover(0)}.querySelector(".paper-check")`);
  await click(`${cover(2)}.querySelector(".cover-open")`, 2);
  const two = await s.js(`document.querySelector(".library-selection strong")?.textContent`);
  await click(`${cover(4)}.querySelector(".cover-open")`, 8);
  const range = await s.js(`document.querySelector(".library-selection strong")?.textContent`);
  await key("Escape", "Escape", 27);
  const none = await count(".library-selection");
  await key("a", "KeyA", 65, 2);
  const all = await s.js(`document.querySelector(".library-selection strong")?.textContent`);
  await key("Escape", "Escape", 27);
  check("several_papers_can_be_selected", two === "2 selected" && range === "4 selected" && none === 0 && all === `${n} selected`, { two, range, none, all });

  // Moving several by menu, and Undo.
  await click(`${cover(0)}.querySelector(".paper-check")`);
  await click(`${cover(1)}.querySelector(".paper-check")`);
  await s.shot("folders-selected.png");
  await click(withText(".library-selection button", "Move to"));
  await click(withText(".ep-menu button", "Work"));
  await s.until(`document.querySelectorAll(".paper-cover").length === ${n - 2} ? true : null`, 4000);
  const moved = await count(".paper-cover");
  await undo();
  const back = await count(".paper-cover");
  check("several_papers_move_and_come_back", moved === n - 2 && back === n, { moved, back });

  // Dragging onto a folder with the mouse; the click after it does not open the paper.
  const from = await at(`${cover(0)}.querySelector(".cover-open")`);
  const to = await at(`document.querySelectorAll(".folder-tile")[0]`);
  await mouse("mousePressed", from.x, from.y);
  for (let i = 1; i <= 10; i++) await mouse("mouseMoved", from.x + ((to.x - from.x) * i) / 10, from.y + ((to.y - from.y) * i) / 10);
  const ghost = await count(".drag-ghost");
  const target = await count(".folder-tile.drop");
  await s.shot("folders-drag.png");
  await mouse("mouseReleased", to.x, to.y);
  await s.until(`document.querySelectorAll(".paper-cover").length === ${n - 1} ? true : null`, 4000);
  check("dragging_moves_a_paper", ghost === 1 && target === 1 && (await count(".paper-cover")) === n - 1 && (await count(".library-view")) === 1, { ghost, target, covers: await count(".paper-cover") });

  // In the folder: its papers and its folders, the way back.
  const name = await s.js(`document.querySelectorAll(".folder-tile .nav-name")[0].textContent`);
  await click(`document.querySelectorAll(".folder-tile .folder-open")[0]`);
  await sleep(500);
  check("a_folder_shows_its_papers_and_the_way_back", (await count(".paper-cover")) === 1 && (await s.js(`document.querySelector(".folder-path")?.innerText.replace(/\\s+/g, " ").trim()`)).includes(name), { name });
  await click(withText(".folder-path button", "Home"));
  await sleep(500);

  // A new folder for a selection.
  await click(`${cover(0)}.querySelector(".paper-check")`);
  await click(withText(".library-selection button", "Move to"));
  await click(withText(".ep-menu button", "New folder"));
  await s.until(`document.querySelector(".prompt-input")`, 3000);
  await type("Made for it");
  await sleep(400);
  check("a_selection_goes_into_a_new_folder", (await tree()).some((x) => /Made for it ?1/.test(x)), { tree: await tree() });

  // Removing several, and Undo.
  const before = await count(".paper-cover");
  await click(`${cover(0)}.querySelector(".paper-check")`);
  await click(`${cover(1)}.querySelector(".paper-check")`);
  await click(withText(".library-selection button", "Remove"));
  await s.until(`document.querySelectorAll(".paper-cover").length === ${before - 2} ? true : null`, 4000);
  const removed = await count(".paper-cover");
  await undo();
  check("several_papers_are_removed_and_come_back", removed === before - 2 && (await count(".paper-cover")) === before, { before, removed });

  // Suggested sorting: shown first, nothing moved until it is approved.
  await key("a", "KeyA", 65, 2);
  await click(withText(".library-selection button", "Suggest folders"));
  await s.until(`document.querySelector(".sort-sheet")`, 4000);
  if (!llm) {
    const says = await s.until(`/No language model/.test(document.querySelector(".sort-sheet").textContent) ? true : null`, 10000);
    check("without_a_model_it_says_so", says === true);
    await key("Escape", "Escape", 27);
  } else {
    const t0 = Date.now();
    const ready = await s.until(`document.querySelector(".sort-group") ? true : null`, 180000);
    const ms = Date.now() - t0;
    const groups = await s.js(`[...document.querySelectorAll(".sort-group")].map((g) => ({ name: g.querySelector("input.sort-name")?.value ?? g.querySelector(".sort-name")?.textContent, papers: [...g.querySelectorAll(".sort-title")].map((e) => e.textContent) }))`);
    await s.shot("folders-suggested.png");
    const untouched = await count(".paper-cover");
    const listed = groups?.flatMap((g) => g.papers).length;
    check("suggestion_changes_nothing_until_approved", ready === true && untouched === before && listed === before, { ms, groups, untouched, listed });
    await s.js(`(() => { const i = document.querySelector("input.sort-name"); if (!i) return; const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set; set.call(i, "My own name"); i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
    await click(withText(".ep-dialog-actions button", "Create folders"));
    await sleep(1200);
    const after = await count(".paper-cover");
    const folders = await tree();
    await undo();
    check("approved_sorting_moves_and_can_be_undone", after < before && folders.some((x) => x.startsWith("My own name")) && (await count(".paper-cover")) === before && !(await tree()).some((x) => x.startsWith("My own name")), { before, after, folders });
  }
  // Deleting a folder keeps its papers.
  await click(q('.library-nav [aria-label="Folder Made for it"]'));
  await click(withText(".ep-menu button", "Delete folder"));
  await sleep(500);
  await s.shot("folders-delete.png");
  const asked = await s.js(`document.querySelector(".ep-dialog")?.innerText ?? document.querySelector(".ep-menu")?.innerText ?? "nothing"`);
  await click(`[...document.querySelectorAll(".ep-dialog-actions button")].at(-1)`);
  await sleep(700);
  check("deleting_a_folder_keeps_the_papers", !(await tree()).some((x) => x.startsWith("Made for it")) && (await count(".paper-cover")) === before + 1, { asked, tree: await tree(), covers: await count(".paper-cover") });
  var last = await tree();
} finally {
  s.app.kill();
}
await sleep(1500);
s = await start([]);
try {
  await s.until(`document.querySelector(".topbar")`, 15000);
  await library();
  const now = await tree();
  check("folders_survive_a_restart", JSON.stringify(now) === JSON.stringify(last) && now.length >= 3, { now });
  // The list view has the same: marks and selection.
  await click(q('.library-tools [aria-label="List"]'));
  await sleep(400);
  await click(`document.querySelectorAll(".paper-item")[0].querySelector(".paper-check")`);
  check("the_list_selects_too", (await s.js(`document.querySelector(".library-selection strong")?.textContent`)) === "1 selected");
  await s.shot("folders-list.png");
} finally {
  s.app.kill();
}
writeFileSync(join(out, "folders.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
