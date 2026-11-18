// End-to-end check of the library and printing (design language §17.9, §17.10).
//
//   node e2e/library.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Library: list and cover views, rename in place, remove with Undo. Printing: Ctrl+P hands the
// paper to the system print (dry run here: no real print job) and never prints the window.

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const papers = [join(root, ".fixtures/papers/arxiv-2111.12066v2.pdf"), join(root, "fixtures/documents/product-manual.pdf")];
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
const data = join(scratch, "library");
const webview = join(scratch, "library-webview");
rmSync(data, { recursive: true, force: true });
rmSync(webview, { recursive: true, force: true });
const s = await launchApp({ exe, args: papers, data, webview, port: 9340, out, consoleLog: [] });
const key = (k, code, vk, modifiers = 0) =>
  s.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: k, code, windowsVirtualKeyCode: vk, modifiers }).then(() => s.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk, modifiers }));

try {
  await s.until(`document.querySelectorAll(".tab").length === 2 ? true : null`, 20000);
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 20000);

  // Printing (design language §17.9): Ctrl+P opens the print sheet; pages 2-4 are drawn and the
  // print layer holds exactly them. What would be printed is checked by printing to PDF through
  // the same path (the system dialog itself is not opened in a test).
  await s.js(`window.__printed = 0; window.print = () => { window.__printed += 1; }`);
  await s.js(`document.querySelector(".pane.focused .scroller").focus()`);
  await key("p", "KeyP", 80, 2);
  const sheet = await s.until(`document.querySelector('.ep-dialog[aria-label="Print"]') ? true : null`, 5000).catch(() => null);
  await s.js(`(() => { const i = document.querySelector(".print-range .ep-input"); i.focus(); i.value = "2-4"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
  await s.js(`[...document.querySelectorAll(".ep-dialog button")].find((b) => b.textContent.trim() === "Print").click()`);
  const asked = await s.until(`window.__printed > 0 ? true : null`, 30000).catch(() => null);
  const layer = await s.js(`document.querySelectorAll(".print-root img.print-page").length`);
  const pdf = await s.send("Page.printToPDF", { preferCSSPageSize: true, printBackground: true });
  const bytes = Buffer.from(pdf.result?.data ?? "", "base64").toString("latin1");
  writeFileSync(join(out, "printed.pdf"), Buffer.from(pdf.result?.data ?? "", "base64"));
  const pages = (bytes.match(/\/Type\s*\/Page[^s]/g) ?? []).length;
  check("print_prints_the_chosen_pages", !!sheet && !!asked && layer === 3 && pages === 3, { sheet: !!sheet, print_dialog_asked: !!asked, layer_pages: layer, printed_pdf_pages: pages });
  await s.js(`window.dispatchEvent(new Event("afterprint"))`);
  await s.until(`document.querySelector(".print-root") ? null : true`, 3000).catch(() => null);

  // Library: covers view with first pages.
  await s.js(`document.querySelector('.topbar [aria-label="Library"]').click()`);
  await s.until(`document.querySelector(".library-view")`);
  await s.js(`document.querySelector('.library-tools [aria-label="Covers"]').click()`);
  const covers = await s.until(`(() => { const i = [...document.querySelectorAll(".paper-cover .cover-page img")]; return i.length >= 2 && i.every((x) => x.complete && x.naturalWidth > 50) ? i.length : null; })()`, 15000).catch(() => null);
  await s.shot("library-covers.png");
  check("covers_view", covers >= 2, { covers });

  // Rename from the paper's menu.
  await s.js(`[...document.querySelectorAll(".paper-cover")].find((c) => c.textContent.includes("Brewline")).querySelector('[aria-label="More for this paper"]').click()`);
  await s.js(`[...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.includes("Rename")).click()`);
  await s.until(`document.querySelector(".rename-field")`);
  await s.js(`(() => { const i = document.querySelector(".rename-field"); i.value = "Grinder manual"; i.dispatchEvent(new InputEvent("input", { bubbles: true })); })()`);
  await key("Enter", "Enter", 13);
  const renamed = await s.until(`[...document.querySelectorAll(".cover-title")].some((x) => x.textContent === "Grinder manual") ? true : null`, 5000).catch(() => null);
  check("rename", !!renamed, {});

  // Remove with the Delete key, then Undo from the toast.
  await s.js(`[...document.querySelectorAll(".paper-cover")].find((c) => c.textContent.includes("Grinder manual")).focus()`);
  await key("Delete", "Delete", 46);
  const gone = await s.until(`document.querySelectorAll(".paper-cover").length === 1 ? true : null`, 5000).catch(() => null);
  const tabsAfter = await s.js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: "workspace.list_papers", input: {}, deadlineMs: null, approved: [] }).then((p) => p.length)`);
  await s.js(`[...document.querySelectorAll(".ep-toast button, [role=status] button")].find((b) => b.textContent.includes("Undo"))?.click()`);
  const back = await s.until(`document.querySelectorAll(".paper-cover").length === 2 ? true : null`, 5000).catch(() => null);
  check("remove_and_undo", !!gone && !!back && tabsAfter === 1, { removed: !!gone, papers_listed_after_remove: tabsAfter, restored: !!back });

  // The list view is remembered across the switch.
  await s.js(`document.querySelector('.library-tools [aria-label="List"]').click()`);
  const list = await s.until(`document.querySelectorAll(".paper-list .paper-item").length || null`, 3000).catch(() => null);
  check("list_view", list === 2, { items: list });
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
