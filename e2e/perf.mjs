// Viewer performance on the real, built app (P1.M2.VW.1–2 budgets).
//
//   node e2e/perf.mjs <reader.exe> <scratch-dir>
//
// For each paper: open it alone, scroll from top to bottom in steps of 80 % of the view (one
// step per animation frame, then a short pause every few steps so pages can draw), and record:
// - long main-thread tasks (budget: none over 100 ms while scrolling the heavy S1 paper),
// - frames slower than 1/30 s during the scroll (smoothness),
// - JavaScript heap and live canvas pixels at the end (memory budget: < 150 MB for 30 pages).

import { spawn } from "node:child_process";
import { rmSync } from "node:fs";
import { join, resolve } from "node:path";

const [exe, scratch] = process.argv.slice(2);
const root = resolve(import.meta.dirname, "..");
const papers = ["arxiv-2511.21915v1.pdf", "arxiv-2106.03711v2.pdf", "plos-pone-0232683.pdf"];
const themes = ["light", "dark"];
const port = 9336;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = {};

for (const theme of themes) for (const name of papers) {
  const data = join(scratch, `perf-${theme}-${name}`);
  rmSync(data, { recursive: true, force: true });
  const app = spawn(exe, [join(root, ".fixtures/papers", name)], {
    env: { ...process.env, EP_DATA_DIR: data, WEBVIEW2_USER_DATA_FOLDER: join(data, "webview"), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --enable-precise-memory-info` },
    stdio: "ignore",
  });
  try {
    let target;
    for (let i = 0; i < 300 && !target; i++) {
      try {
        target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
      } catch {}
      if (!target) await sleep(50);
    }
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r) => ws.addEventListener("open", r));
    let id = 0;
    const pending = new Map();
    ws.addEventListener("message", (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id && pending.has(msg.id)) {
        pending.get(msg.id)(msg);
        pending.delete(msg.id);
      }
    });
    const js = (expr) =>
      new Promise((r) => {
        const i = ++id;
        pending.set(i, (msg) => r(msg.result?.result?.value ?? { error: msg.result?.exceptionDetails?.exception?.description }));
        ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression: expr, awaitPromise: true, returnByValue: true } }));
      });
    const t0 = Date.now();
    for (let i = 0; i < 400; i++) {
      if (await js(`document.querySelectorAll(".pane.focused canvas.page-canvas, .pane.focused img.page-canvas").length > 0`)) break;
      await sleep(25);
    }
    const firstPage = Date.now() - t0;
    // Pick the theme in the theme menu, as a user would, and let the pages redraw.
    await js(`(() => { document.querySelector('.topbar [aria-label="Theme"]').click(); [...document.querySelectorAll(".ep-menu-item")].find((b) => b.textContent.trim().toLowerCase() === "${theme}").click(); })()`);
    await sleep(2000);
    results[`${theme} ${name}`] = await js(`(async () => {
      const longTasks = [];
      const obs = new PerformanceObserver((l) => l.getEntries().forEach((e) => longTasks.push(Math.round(e.duration))));
      try { obs.observe({ type: "longtask", buffered: false }); } catch {}
      const sc = document.querySelector(".pane.focused .scroller");
      const frames = [];
      let last = performance.now();
      const pages = Number(document.querySelector(".page-box").parentElement.textContent.match(/\\/\\s*(\\d+)/)[1]);
      const started = performance.now();
      let steps = 0;
      while (sc.scrollTop + sc.clientHeight < sc.scrollHeight - 2) {
        sc.scrollTop += sc.clientHeight * 0.8;
        await new Promise((r) => requestAnimationFrame(r));
        const now = performance.now(); frames.push(now - last); last = now;
        if (++steps % 5 === 0) { await new Promise((r) => setTimeout(r, 120)); last = performance.now(); }
      }
      const scrollMs = performance.now() - started;
      await new Promise((r) => setTimeout(r, 1500));
      obs.disconnect();
      const canvases = [...document.querySelectorAll("canvas.page-canvas")];
      const images = document.querySelectorAll("img.page-canvas").length;
      const px = canvases.reduce((a, c) => a + c.width * c.height, 0);
      const slow = frames.filter((f) => f > 33.4).length;
      return {
        pages,
        scroll_steps: steps,
        scroll_ms: Math.round(scrollMs),
        frames_over_33ms: slow,
        frame_p95_ms: Math.round([...frames].sort((a, b) => a - b)[Math.floor(frames.length * 0.95)] ?? 0),
        long_tasks: longTasks.length,
        longest_task_ms: Math.max(0, ...longTasks),
        mounted_pages: document.querySelectorAll(".pane.focused .page").length,
        drawn_by: images > 0 ? "pdfium" : "pdf.js",
        canvas_megapixels: Math.round(px / 1e5) / 10,
        canvas_mb: Math.round((px * 4) / 1e6),
        js_heap_mb: performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1e6) : null,
      };
    })()`);
    results[`${theme} ${name}`].first_page_ms = firstPage;
    ws.close();
  } catch (e) {
    results[`${theme} ${name}`] = { error: String(e) };
  } finally {
    app.kill();
    await sleep(800);
  }
}
console.log(JSON.stringify(results, null, 2));
