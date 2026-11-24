// End-to-end check of the automatic model search in Settings (engines/inference/CONTRACT.md).
//
//   node e2e/models.mjs <reader.exe> <scratch-dir> <out-dir>
//
// Made-up GGUF files in sub-folders: found, suggested, chosen per task, remembered after a restart,
// forgotten. With EP_LLM_PARENT (a folder with real models somewhere below it): the real ones too.

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { launch as launchApp, sleep } from "./lib.mjs";

const [exe, scratch, out] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const report = {};
const failures = [];
const check = (name, ok, detail) => {
  report[name] = { ok, ...detail };
  if (!ok) failures.push(name);
};
mkdirSync(out, { recursive: true });
delete process.env.EP_LLM_DIR;
const data = join(scratch, "models");
const library = join(scratch, "models-library");
for (const d of [data, library, join(scratch, "models-webview")]) rmSync(d, { recursive: true, force: true });

const text = (s) => Buffer.concat([u64(Buffer.byteLength(s)), Buffer.from(s)]);
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
function gguf(path, arch, name, chat) {
  const pairs = [["general.architecture", arch], ["general.name", name], ...(chat ? [["tokenizer.chat_template", "{{ m }}"]] : [])];
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, Buffer.concat([Buffer.from("GGUF"), u32(3), u64(0), u64(pairs.length), ...pairs.flatMap(([k, v]) => [text(k), u32(8), text(v)])]));
}
gguf(join(library, "a/first-model.gguf"), "llama", "First Model With A Rather Long Name For A Narrow Window 7B Instruct", true);
gguf(join(library, "a/deep/er/second-model.gguf"), "qwen3", "Second Model", true);
gguf(join(library, "vision/mmproj-part.gguf"), "clip", "Part", false);
mkdirSync(join(library, "hf"), { recursive: true });
writeFileSync(join(library, "hf/model.safetensors"), "x");

const start = () => launchApp({ exe, args: [join(root, "fixtures/documents/product-manual.pdf")], data, webview: join(scratch, "models-webview"), port: 9392, out, consoleLog: [] });
const settings = async (s) => {
  for (const type of ["rawKeyDown", "keyUp"]) await s.send("Input.dispatchKeyEvent", { type, key: ",", code: "Comma", windowsVirtualKeyCode: 188, modifiers: 2 });
  await s.until(`document.querySelector(".llm") ? true : null`, 5000);
  await s.until(`document.querySelector(".llm-searching") ? null : true`, 15000);
};
const SEEN = `(() => ({
  folders: [...document.querySelectorAll(".llm .model-list")][0].innerText,
  models: [...document.querySelectorAll(".llm .model-list:not(.llm-order) .model-row strong")].map((e) => e.textContent),
  states: [...document.querySelectorAll(".llm .model-list .model-state")].map((e) => e.textContent.trim()),
  order: [...document.querySelectorAll(".llm-order .model-row")].map((e) => [...e.querySelectorAll(".model-text > *")].map((x) => x.textContent)),
}))()`;

let s = await start();
try {
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 20000);
  const added = await s.call("inference.add_folder", { path: library });
  check("folder_is_searched_with_its_sub_folders", added?.models?.length === 3, { error: added?.error, found: added?.models?.map((m) => m.id) });
  await settings(s);
  const seen = await s.js(SEEN);
  await s.shot("models-settings.png");
  check("models_are_listed_and_suggested", seen.models?.length === 3 && seen.states.filter((x) => x === "Suggested").length === 1 && seen.states.includes("Picture part of another model") && /3 models found/.test(seen.folders) && /1 files in other formats/.test(seen.folders), seen);
  const first = seen.order?.[0]?.[1];
  check("the_order_of_the_models_is_shown", seen.order?.length === 2 && seen.order[0][0] === "First choice" && seen.order[1][0] === "If that does not work" && seen.order.every((x) => /needs about/.test(x[2])), { order: seen.order });
  // The second one first.
  await s.js(`document.querySelectorAll('.llm-order [aria-label^="Earlier"]')[1].click()`);
  const swapped = await s.until(`document.querySelectorAll(".llm-order strong")[1]?.textContent === ${JSON.stringify(first)} && !document.querySelector(".llm-searching") ? true : null`, 8000);
  check("the_order_can_be_changed", swapped === true, { order: (await s.js(SEEN)).order });
  // Left out, and taken in again.
  await s.js(`document.querySelectorAll('.llm-order [aria-label^="Leave out"]')[1].click()`);
  const one = await s.until(`document.querySelectorAll(".llm-order .model-row").length === 1 && !document.querySelector(".llm-searching") ? true : null`, 8000);
  await s.js(`(() => { const e = document.querySelector(".llm select"); e.value = e.options[1].value; e.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  const two = await s.until(`document.querySelectorAll(".llm-order .model-row").length === 2 && !document.querySelector(".llm-searching") ? true : null`, 8000);
  check("a_model_can_be_left_out_and_added", one === true && two === true);
  // Not kept ready: made-up models would not start anyway.
  await s.js(`(() => { const e = [...document.querySelectorAll(".llm .settings-row")].find((x) => /keep it ready/.test(x.textContent)).querySelector("input"); e.click(); })()`);
  await s.until(`document.querySelector(".llm-searching") ? null : true`, 8000);
  await s.shot("models-order.png");
  var chosen = first;
  await sleep(300);
} finally {
  s.app.kill();
}
await sleep(1500);
s = await start();
try {
  await s.until(`document.querySelector(".pane.focused img.page-canvas")`, 20000);
  await settings(s);
  const seen = await s.js(SEEN);
  const keep = await s.js(`[...document.querySelectorAll(".llm .settings-row")].find((x) => /keep it ready/.test(x.textContent)).querySelector("input").checked`);
  check("choices_survive_a_restart", seen.order?.length === 2 && seen.order[1][1] === chosen && keep === false && seen.models.length >= 3, { order: seen.order, keep, models: seen.models });
  await s.js(`[...document.querySelectorAll(".llm button")].find((b) => b.textContent.trim() === "Forget").click()`);
  const gone = await s.until(`document.querySelectorAll(".llm .model-row").length === 0 && !document.querySelector(".llm-searching") ? true : null`, 8000);
  const says = await s.js(`document.querySelector(".llm").innerText`);
  check("a_folder_can_be_forgotten", gone === true && /No model that writes answers/.test(says));
  if (process.env.EP_LLM_PARENT) {
    const t0 = Date.now();
    const m = await s.call("inference.add_folder", { path: process.env.EP_LLM_PARENT });
    const ms = Date.now() - t0;
    const chat = (m?.models ?? []).filter((x) => x.kind === "chat");
    await settings(s).catch(() => {});
    await s.shot("models-real.png");
    check("real_models_are_found_below_the_folder", chat.length > 0 && chat.some((x) => x.recommended) && ms < 8000, { ms, chat: chat.map((x) => `${x.name}${x.recommended ? " (suggested)" : ""}`), other: m?.models?.filter((x) => x.kind !== "chat").map((x) => `${x.name}: ${x.kind}`), formats: m?.folders?.[0]?.other_formats });
  }
} finally {
  s.app.kill();
}
writeFileSync(join(out, "models.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
console.log(failures.length === 0 ? "PASS" : `FAIL: ${failures.join(", ")}`);
process.exit(failures.length === 0 ? 0 : 1);
