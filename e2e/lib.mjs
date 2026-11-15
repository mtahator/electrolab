// Shared driver for the end-to-end scripts: start the built Reader with a private data folder and
// WebView2's DevTools port, and talk to the page over the DevTools protocol.

import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Start the app. `consoleLog` collects page errors and warnings. */
export async function launch({ exe, args, data, webview, port, out, consoleLog }) {
  const app = spawn(exe, args, {
    env: { ...process.env, EP_DATA_DIR: data, WEBVIEW2_USER_DATA_FOLDER: webview, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
    stdio: "ignore",
  });
  let target;
  for (let i = 0; i < 300 && !target; i++) {
    try {
      target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
    } catch {}
    if (!target) await sleep(50);
  }
  if (!target) throw new Error("no DevTools page");
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r));
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    } else if (msg.method === "Runtime.exceptionThrown") {
      consoleLog.push(`exception: ${(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text).slice(0, 300)}`);
    } else if (msg.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(msg.params.type)) {
      const text = msg.params.args.map((a) => a.value ?? a.description).join(" ");
      if (!text.startsWith("IPC custom protocol")) consoleLog.push(`${msg.params.type}: ${text.slice(0, 300)}`);
    }
  });
  const send = (method, params = {}) =>
    new Promise((r) => {
      const i = ++id;
      pending.set(i, r);
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  void send("Runtime.enable");
  const js = async (expr) => {
    const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) return { error: r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text };
    return r.result?.result?.value;
  };
  const until = async (expr, ms = 15000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const v = await js(expr);
      if (v && !v.error) return v;
      await sleep(50);
    }
    return null;
  };
  const shot = async (name) => {
    const r = await send("Page.captureScreenshot", { format: "png" });
    if (r.result?.data) writeFileSync(join(out, name), Buffer.from(r.result.data, "base64"));
  };
  const keyOn = (target, k, mods = {}) =>
    js(`${target}.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(k)}, ctrlKey: ${!!mods.ctrl}, altKey: ${!!mods.alt}, shiftKey: ${!!mods.shift}, bubbles: true }))`);
  const key = (k, mods) => keyOn("window", k, mods);
  const scrollerKey = (k, mods) => keyOn(`document.querySelector(".pane.focused .scroller")`, k, mods);
  /** A bus call from the page, as the UI makes it. */
  const call = (command, input) => js(`window.__TAURI_INTERNALS__.invoke("bus_call", { command: ${JSON.stringify(command)}, input: ${JSON.stringify(input)}, deadlineMs: null, approved: [] })`);
  return { app, ws, js, until, shot, key, scrollerKey, call, send };
}
