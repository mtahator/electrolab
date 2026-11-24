// End-to-end check that everything fits (design language §5.4): every part of the interface, at
// the normal and the smallest window, at 90, 100 and 150 % text. Nothing spills out of its box,
// no control is cut off, nothing leaves the window, controls do not touch.
//
//   node e2e/layout.mjs <reader.exe> <scratch-dir> <out-dir>

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  const r = (report[name] ??= { ok: true, cases: 0, wrong: [] });
  r.cases++;
  if (ok) return;
  r.ok = false;
  r.wrong.push(detail);
  if (!failures.includes(name)) failures.push(name);
};
mkdirSync(out, { recursive: true });
const data = join(scratch, "layout");
const webview = join(scratch, "layout-webview");
rmSync(data, { recursive: true, force: true });
rmSync(webview, { recursive: true, force: true });
delete process.env.EP_LLM_DIR;
const papers = [join(root, "fixtures/documents/product-manual.pdf"), join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf")];
const s = await launchApp({ exe, args: papers, data, webview, port: 9390, out, consoleLog: [] });
const key = async (k, code, vk, modifiers = 0) => {
  for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: vk, modifiers });
};
const escape = () => key("Escape", "Escape", 27);
const click = (sel, text) => s.js(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(sel)})].find((x) => ${text ? `x.textContent.trim().startsWith(${JSON.stringify(text)})` : "true"}); e?.click(); return !!e; })()`);
const bar = (label) => click(`.topbar [aria-label^="${label}"]`);
const has = (sel, ms = 4000) => s.until(`document.querySelector(${JSON.stringify(sel)}) ? true : null`, ms);
const gone = (sel) => s.until(`document.querySelector(${JSON.stringify(sel)}) ? null : true`, 3000);
const selectWords = () =>
  s.js(`(() => { const w = [...document.querySelectorAll(".pane.focused .text-layer span[data-page]")].find((e) => { const r = e.getBoundingClientRect(); return r.top > 120 && r.bottom < innerHeight - 60 && r.width > 10; });
    if (!w) return false; const r = document.createRange(); r.selectNodeContents(w); const g = getSelection(); g.removeAllRanges(); g.addRange(r);
    document.querySelector(".pane.focused .scroller").dispatchEvent(new MouseEvent("mouseup", { bubbles: true })); return true; })()`);

// covers: App.tsx (menus, model question, address, confirm), shell/Settings.tsx, shell/LanguageModels.tsx, shell/Print.tsx, shell/Overlays.tsx (palette, shortcuts, address)
// covers: shell/BrainMark.tsx, shell/LibraryView.tsx, shell/LibraryFolders.tsx, shell/SortSheet.tsx, shell/AskView.tsx, shell/SummarySheet.tsx, shell/SidePanel.tsx, assistant/ExplainCard.tsx, annotations/SelectionBar.tsx, annotations/ToolRail.tsx, annotations/Editor.tsx, annotations/AnnotationLayer.tsx
// covers: annotations/AnnotationsPanel.tsx, intel/Peek.tsx, intel/TableView.tsx, intel/PageLinks.tsx, notes/NotesPanel.tsx, viewer/Viewer.tsx, annotations/MarginNotes.tsx
/** Parts of the interface: open it (true when it is there), close it. */
const PARTS = {
  reading: { open: () => has(".pane.focused img.page-canvas") },
  ...Object.fromEntries(
    [0, 1, 2, 3, 4].map((i) => [
      `side_panel_${i}`,
      { open: () => s.js(`(() => { if (!document.querySelector(".side.left")) document.querySelector('.topbar [aria-label^="Side panel"]').click(); const b = document.querySelectorAll(".side.left .ep-panel button.ep-icon-button")[${i}]; b?.click(); return !!b; })()`) },
    ]),
  ),
  notes: { open: async () => (await bar("Notes"), has(".side.right, aside[aria-label*='otes']")), close: () => bar("Notes") },
  find: { open: async () => (await key("f", "KeyF", 70, 2), has(".find-bar")), close: escape },
  split: { open: async () => (await bar("Split view"), sleep(500), has(".pane")), close: () => bar("Split view") },
  more_menu: { open: async () => (await bar("More"), has(".ep-menu")), close: escape },
  theme_menu: { open: async () => (await bar("Theme"), has(".ep-menu")), close: escape },
  commands: { open: async () => (await bar("Commands"), has(".palette")), close: escape },
  selection_bar: { open: async () => (await selectWords(), has(".selection-bar")), close: () => s.js("getSelection().removeAllRanges()") },
  explain: {
    open: async () => {
      await selectWords();
      await has(".selection-bar");
      await click('.selection-bar [aria-label^="Explain"]');
      return has(".explain .ep-card", 8000);
    },
    close: async () => (await escape(), s.js("getSelection().removeAllRanges()")),
  },
  settings: { open: async () => (await key(",", "Comma", 188, 2), has(".ep-dialog .settings")), close: escape },
  shortcuts: {
    open: async () => {
      await bar("More");
      await has(".ep-menu");
      await click(".ep-menu button", "Keyboard shortcuts");
      return has(".ep-dialog .keymap");
    },
    close: escape,
  },
  print: { open: async () => (await s.js(`document.querySelector(".pane.focused .scroller").focus()`), await key("p", "KeyP", 80, 2), has(".ep-dialog .print-sheet")), close: escape },
  model_question: { open: async () => (await click(".ep-status-item", "Recognise"), has(".ep-dialog")), close: escape },
  address: { open: async () => (await bar("Open URL"), has(".ep-dialog .prompt-input")), close: escape },
  tool_flyout: { open: async () => (await s.js(`document.querySelector('.tool-rail [aria-label^="Highlight"]').click()`), sleep(200), true), close: async () => (await escape(), escape()) },
  library_list: { open: async () => (await bar("Library"), await has(".library-view"), await click('.library-tools [aria-label="List"]'), has(".paper-item")), keep: true },
  library_selection: { open: async () => (await has(".library-view"), await s.js(`document.querySelector(".paper-check").click()`), has(".library-selection")), close: () => s.js(`[...document.querySelectorAll(".library-selection button")].at(-1).click()`) },
  folder_menu: { open: async () => (await click(".library-nav .nav-item", "All papers"), await sleep(350), await click('.library-nav [aria-label^="Folder "]'), has(".ep-menu")), close: escape },
  move_menu: { open: async () => (await s.js(`document.querySelector(".paper-check").click()`), await click(".library-selection button", "Move to"), has(".ep-menu")), close: async () => (await escape(), s.js(`[...document.querySelectorAll(".library-selection button")].at(-1)?.click()`)) },
  suggested_folders: { open: async () => (await s.js(`document.querySelector(".paper-check").click()`), await click(".library-selection button", "Suggest"), has(".sort-sheet")), close: async () => (await escape(), s.js(`[...document.querySelectorAll(".library-selection button")].at(-1)?.click()`)) },
  ask: { open: async () => (await click(".library-nav .nav-ask"), has(".ask-box")), keep: true },
  ask_results: {
    open: async () => {
      await click(".library-nav .nav-ask");
      await has(".ask-box textarea");
      await s.js(`document.querySelector(".ask-box textarea").focus()`);
      await key("a", "KeyA", 65, 2);
      await s.send("Input.insertText", { text: "table with the recommended settings of the grinder, or a figure of the velocity near the wall" });
      await key("Enter", "Enter", 13);
      return has(".ask-results .ask-place", 8000);
    },
    keep: true,
  },
  paper_summary: {
    open: async () => {
      await has(".ask-paper-head button");
      await click(".ask-paper-head button", "Summary");
      return has(".summary-sheet .summary-section", 8000);
    },
    close: async () => (await escape(), click(".library-nav .nav-item", "All papers")),
  },
  library_covers: { open: async () => (await click('.library-tools [aria-label="Covers"]'), has(".paper-covers")), close: () => bar("Library") },
};

const AUDIT = `(() => {
  const spill = [], cut = [], off = [], touching = [];
  const name = (e) => (e.getAttribute("aria-label") || e.className?.toString?.().split(" ").slice(0, 2).join(".") || e.tagName) + (e.children.length ? "" : ":" + e.textContent.trim().slice(0, 24));
  const boxed = (e) => { const c = getComputedStyle(e); return (c.backgroundColor !== "rgba(0, 0, 0, 0)" && c.backgroundColor !== "transparent") || parseFloat(c.borderRightWidth) > 0; };
  const control = (e) => e.matches("button, select, input, textarea, [role=button]");
  for (const e of document.querySelectorAll("body *")) {
    if (e.closest("svg, .scroller, .ep-sr-only, [aria-hidden=true], .print-root, option")) continue;
    const r = e.getBoundingClientRect();
    if (r.width < 2 || r.height < 2 || getComputedStyle(e).visibility === "hidden" || getComputedStyle(e).opacity === "0") continue;
    const style = getComputedStyle(e);
    let box = null, scrolls = false, hidden = null;
    if (style.position !== "fixed") for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
      const c = getComputedStyle(a);
      if (/(auto|scroll)/.test(c.overflowX) && a.scrollWidth > a.clientWidth + 1 && a.matches(".tab-strip, .scroll-x, table, .table-view *")) { scrolls = true; break; }
      const ar = a.getBoundingClientRect();
      if (!hidden && c.overflowX !== "visible" && (r.right > ar.right + 1.5 || r.left < ar.left - 1.5)) hidden = a;
      if (!box && boxed(a)) box = a;
      if (c.position === "fixed") break;
    }
    if (scrolls) continue;
    if (r.right > innerWidth + 1 || r.left < -1) off.push(name(e) + " " + Math.round(r.left) + ".." + Math.round(r.right));
    if (hidden && control(e)) cut.push(name(e) + " in " + name(hidden));
    if (box && !hidden && style.position !== "absolute") { const b = box.getBoundingClientRect(); if (r.right > b.right + 1.5 || r.left < b.left - 1.5) spill.push(name(e) + " out of " + name(box) + " by " + Math.round(Math.max(r.right - b.right, b.left - r.left))); }
    if (control(e) && e.scrollWidth > e.clientWidth + 2 && style.textOverflow !== "ellipsis" && e.matches("button")) cut.push("text of " + name(e));
    if (style.position === "fixed" && e.matches(".ep-card, .ep-menu, .selection-bar, .palette, .peek, .explain") && (r.top < -1 || r.bottom > innerHeight + 1)) off.push(name(e) + " vertical " + Math.round(r.top) + ".." + Math.round(r.bottom));
  }
  const d = document.querySelector(".ep-dialog");
  if (d) {
    const r = d.getBoundingClientRect();
    if (r.top < 0 || r.bottom > innerHeight) off.push("dialog vertical " + Math.round(r.top) + ".." + Math.round(r.bottom));
    for (const b of d.querySelectorAll(".ep-dialog-actions button")) { const x = b.getBoundingClientRect(); if (x.bottom > r.bottom || x.right > r.right) cut.push("dialog button " + name(b)); }
    const cs = [...d.querySelectorAll(".ep-dialog-body select, .ep-dialog-body button, .ep-dialog-body .model-row")].filter((e) => !e.parentElement.closest(".model-row, table")).map((e) => e.getBoundingClientRect());
    for (let i = 1; i < cs.length; i++) { const gap = cs[i].top - cs[i - 1].bottom; if (gap < 7.5 && cs[i].top > cs[i - 1].top + 4) touching.push(gap.toFixed(1)); }
  }
  const u = (a) => [...new Set(a)].slice(0, 6);
  const panes = document.querySelector(".panes")?.getBoundingClientRect().width ?? null;
  return { spill: u(spill), cut: u(cut), off: u(off), touching: u(touching), panes };
})()`;

try {
  await s.until(`document.querySelectorAll(".pane.focused .text-layer span[data-page]").length > 15 ? true : null`, 25000);
  await sleep(1500);
  // The longest texts: models with long names, in a folder with a long path.
  const models = join(scratch, "layout-models", "a-folder-with-a-long-name-that-cannot-be-broken-anywhere-at-all");
  mkdirSync(models, { recursive: true });
  const text = (t) => Buffer.concat([u64(Buffer.byteLength(t)), Buffer.from(t)]);
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
  const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
  for (const i of [1, 2, 3]) {
    const pairs = [["general.architecture", "llama"], ["general.name", `A Very Long Model Name Number ${i} Instruct Distilled Long Context Preview`], ["tokenizer.chat_template", "x"]];
    writeFileSync(join(models, `A_Very_Long_Model_File_Name_${i}-Instruct-Distilled-Long-Context-Preview-Q4_K_M.gguf`), Buffer.concat([Buffer.from("GGUF"), u32(3), u64(0), u64(3), ...pairs.flatMap(([k, v]) => [text(k), u32(8), text(v)])]));
  }
  await s.call("inference.add_folder", { path: models });
  const top = await s.call("workspace.create_collection", { name: "A folder with a rather long name that has to be shortened" });
  await s.call("workspace.create_collection", { name: "Another-long-name-without-any-place-to-break-it-at-all", parent: top?.id });
  for (const [w, h] of [[1400, 950], [1024, 640], [800, 500]]) {
    await s.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
    for (const size of ["100", "90", "150"]) {
      await key(",", "Comma", 188, 2);
      await has(".ep-dialog .settings");
      await s.js(`(() => { const e = [...document.querySelectorAll(".settings select")].at(2); e.value = "${size}"; e.dispatchEvent(new Event("change", { bubbles: true })); })()`);
      await escape();
      await sleep(400);
      for (const [name, part] of Object.entries(PARTS)) {
        const id = `${name} ${w}x${h} ${size}%`;
        const there = await part.open();
        check("every_part_opens", there === true, id);
        await sleep(380);
        const m = await s.js(AUDIT);
        check("nothing_spills_out_of_its_box", m.spill?.length === 0, { id, spill: m.spill, error: m.error });
        check("no_control_is_cut_off", m.cut?.length === 0, { id, cut: m.cut });
        check("nothing_leaves_the_window", m.off?.length === 0, { id, off: m.off });
        if (m.panes !== null) check("paper_keeps_room", m.panes >= 319, { id, panes: String(m.panes) });
        check("controls_do_not_touch", m.touching?.length === 0, { id, touching: m.touching });
        if (size !== "90" && (w === 800 || (w === 1400 && size === "100"))) await s.shot(`layout-${name}-${w}-${size}.png`);
        await part.close?.();
        if (["settings", "shortcuts", "print", "model_question", "address", "suggested_folders", "paper_summary"].includes(name)) await gone(".ep-dialog");
        await sleep(120);
      }
    }
  }
} finally {
  s.app.kill();
}
writeFileSync(join(out, "layout.json"), JSON.stringify(report, null, 2));
for (const [n, r] of Object.entries(report)) {
  console.log(`${r.ok ? "ok  " : "FAIL"} ${n}: ${r.cases} cases, ${r.wrong.length} wrong`);
  for (const x of r.wrong.slice(0, 400)) console.log("   ", typeof x === "string" ? x : Object.values(x).filter(Boolean).map((v) => (Array.isArray(v) ? v.join(" | ") : v)).join(" → "));
}
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
