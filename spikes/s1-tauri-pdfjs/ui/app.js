// Spike P0.S1 — virtualised PDF.js viewer with measurements. Throw-away code.
import * as pdfjsLib from "./vendor/pdfjs/pdf.mjs";

pdfjsLib.GlobalWorkerOptions.workerSrc = "./vendor/pdfjs/pdf.worker.mjs";
window.__spikeStarted = true;
const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);
const scroller = $("scroller");
const viewer = $("viewer");
const status = (s) => ($("status").textContent = s);

const CHUNK = 65536;
const epUrl = (path) => `http://ep.localhost/file?path=${encodeURIComponent(path)}`;

// ---- loading through the ep protocol with range requests -------------------------------
class EpTransport extends pdfjsLib.PDFDataRangeTransport {
  constructor(url, length, initial) {
    super(length, initial);
    this.url = url;
  }
  requestDataRange(begin, end) {
    fetch(this.url, { headers: { Range: `bytes=${begin}-${end - 1}` } })
      .then((r) => r.arrayBuffer())
      .then((b) => this.onDataRange(begin, new Uint8Array(b)));
  }
}

const docOptions = {
  rangeChunkSize: CHUNK,
  disableAutoFetch: true,
  cMapUrl: "./vendor/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "./vendor/pdfjs/standard_fonts/",
  wasmUrl: "./vendor/pdfjs/wasm/",
  iccUrl: "./vendor/pdfjs/iccs/",
  isEvalSupported: false,
  enableXfa: false,
};

// ---- viewer state ------------------------------------------------------------------------
let doc = null, task = null, pages = [], tops = [], scale = 1, gen = 0;
const queue = new Set();
const phases = []; // per rendered page: wall-clock spans of canvas drawing and text-layer building
let rendering = 0;
const MAX_PARALLEL = 2, PREFETCH = 2, KEEP = 6;
const dpr = () => window.devicePixelRatio || 1;

async function open(path) {
  const myGen = ++gen;
  if (task) await task.destroy();
  doc = null; pages = []; tops = []; queue.clear(); viewer.replaceChildren();
  const t = { start: performance.now() };
  const info = await invoke("doc_info", { path });                     // page sizes from PDFium
  t.sizes = performance.now();
  const url = epUrl(path);
  const first = new Uint8Array(await (await fetch(url, { headers: { Range: `bytes=0-${Math.min(CHUNK, info.bytes) - 1}` } })).arrayBuffer());
  task = pdfjsLib.getDocument({ ...docOptions, range: new EpTransport(url, info.bytes, first), length: info.bytes });
  layout(info.pages);                                                   // whole document laid out before parsing
  t.layout = performance.now();
  doc = await task.promise;
  if (myGen !== gen) return null;
  t.parsed = performance.now();
  const done = renderPage(0, myGen);
  update();
  const { canvasAt, textAt } = await done;
  t.canvas = canvasAt; t.text = textAt;
  return { info, t };
}

function layout(sizes) {
  const maxW = Math.max(...sizes.map(([w]) => w));
  scale = Math.min(2, (scroller.clientWidth - 40) / maxW);
  const gap = 12;
  let y = gap;
  const frag = document.createDocumentFragment();
  pages = sizes.map(([w, h], i) => {
    const div = document.createElement("div");
    div.className = "page empty";
    div.dataset.pageNumber = String(i + 1);
    div.style.width = `${Math.floor(w * scale)}px`;
    div.style.height = `${Math.floor(h * scale)}px`;
    div.style.setProperty("--scale-factor", String(scale));
    frag.append(div);
    tops.push(y);
    y += Math.floor(h * scale) + gap;
    return { div, state: "empty", canvas: null, text: null, task: null };
  });
  viewer.append(frag);
}

function visibleRange() {
  const top = scroller.scrollTop, bottom = top + scroller.clientHeight;
  let lo = 0, hi = tops.length - 1;
  while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (tops[mid] <= top) lo = mid; else hi = mid - 1; }
  let last = lo;
  while (last + 1 < tops.length && tops[last + 1] < bottom) last++;
  return [lo, last];
}

function update() {
  if (!doc) return;
  const [a, b] = visibleRange();
  // visible pages first (top to bottom), then prefetch below, then above
  const want = [];
  for (let i = a; i <= b; i++) want.push(i);
  for (let i = b + 1; i <= Math.min(pages.length - 1, b + PREFETCH); i++) want.push(i);
  for (let i = a - 1; i >= Math.max(0, a - 1); i--) want.push(i);
  queue.clear();
  for (const i of want) if (pages[i].state === "empty") queue.add(i);
  // cancel work far away, free memory further away
  pages.forEach((p, i) => {
    const far = i < a - PREFETCH - 1 || i > b + PREFETCH + 1;
    if (far && p.state === "rendering" && p.task) p.task.cancel();
    if ((i < a - KEEP || i > b + KEEP) && p.state === "done") release(i);
  });
  pump();
}

function pump() {
  const myGen = gen;
  while (rendering < MAX_PARALLEL && queue.size) {
    const i = queue.values().next().value;
    queue.delete(i);
    renderPage(i, myGen).catch(() => {}).finally(() => pump());
  }
}

async function renderPage(i, myGen) {
  const p = pages[i];
  if (p.state !== "empty") return {};
  p.state = "rendering";
  rendering++;
  try {
    const page = await doc.getPage(i + 1);
    const viewport = page.getViewport({ scale });
    const canvas = document.createElement("canvas");
    const out = dpr();
    canvas.width = Math.floor(viewport.width * out);
    canvas.height = Math.floor(viewport.height * out);
    const r0 = performance.now();
    p.task = page.render({ canvasContext: canvas.getContext("2d", { alpha: false }), viewport,
      transform: out !== 1 ? [out, 0, 0, out, 0, 0] : null });
    await p.task.promise;
    if (myGen !== gen) return {};
    p.div.append(canvas);
    p.div.classList.remove("empty");
    p.canvas = canvas;
    const canvasAt = performance.now();
    const text = document.createElement("div");
    text.className = "textLayer";
    p.div.append(text);
    p.text = text;
    await new pdfjsLib.TextLayer({ textContentSource: page.streamTextContent({ includeMarkedContent: true, disableNormalization: true }), container: text, viewport }).render();
    phases.push({ page: i + 1, canvas: [r0, canvasAt], text: [canvasAt, performance.now()], spans: text.childElementCount });
    p.state = "done";
    return { canvasAt, textAt: performance.now() };
  } catch (e) {
    p.state = "empty";
    if (e?.name !== "RenderingCancelledException") console.error(e);
    return {};
  } finally {
    p.task = null;
    rendering--;
  }
}

function release(i) {
  const p = pages[i];
  if (p.canvas) { p.canvas.width = 0; p.canvas.height = 0; p.canvas.remove(); }
  p.text?.remove();
  p.canvas = p.text = null;
  p.state = "empty";
  p.div.classList.add("empty");
}

scroller.addEventListener("scroll", () => update(), { passive: true });

// ---- measurements --------------------------------------------------------------------------
const raf = () => new Promise((r) => requestAnimationFrame(r));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const q = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s.length ? +s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(2) : null; };

function rel(t) {
  return { sizes_ms: +(t.sizes - t.start).toFixed(1), layout_ms: +(t.layout - t.start).toFixed(1), parsed_ms: +(t.parsed - t.start).toFixed(1),
    first_canvas_ms: +(t.canvas - t.start).toFixed(1), first_text_layer_ms: +(t.text - t.start).toFixed(1) };
}

async function frameRate(ms = 1000) {
  const ts = [];
  const end = performance.now() + ms;
  while (performance.now() < end) ts.push(await raf());
  const d = ts.slice(1).map((x, i) => x - ts[i]);
  return { idle_frame_ms_median: q(d, 0.5), idle_fps: +(1000 / q(d, 0.5)).toFixed(1) };
}

async function benchOpen(path) {
  await invoke("protocol_stats", { reset: true });
  const cold = await open(path);
  const [requests, bytes] = await invoke("protocol_stats", { reset: true });
  const warm = [];
  for (let k = 0; k < 5; k++) { await sleep(300); warm.push(rel((await open(path)).t)); await invoke("protocol_stats", { reset: true }); }
  return { pages: cold.info.pages.length, file_bytes: cold.info.bytes, pdfium_sizes_ms: +cold.info.pdfium_ms.toFixed(1),
    cold: rel(cold.t), warm, warm_first_canvas_ms_median: q(warm.map((w) => w.first_canvas_ms), 0.5),
    warm_first_text_layer_ms_median: q(warm.map((w) => w.first_text_layer_ms), 0.5),
    cold_requests: requests, cold_bytes_fetched: bytes, cold_bytes_share: +(bytes / cold.info.bytes).toFixed(3),
    selection: await selectionCheck(), ...(await frameRate()) };
}

async function benchScroll(path, maxPages, speed) {
  await open(path);
  await sleep(800);
  const lastPage = Math.min(pages.length - 1, maxPages - 1);
  const target = tops[lastPage];
  const longTasks = [], longAt = [];
  phases.length = 0;
  const po = new PerformanceObserver((l) => l.getEntries().forEach((e) => { longTasks.push(e.duration); longAt.push([e.startTime, e.duration]); }));
  try { po.observe({ type: "longtask", buffered: false }); } catch {}
  const frames = [], blank = [];
  let t0 = await raf(), prev = t0;
  scroller.scrollTop = 0;
  for (;;) {
    const now = await raf();
    frames.push(now - prev); prev = now;
    const y = Math.min(target, speed * (now - t0) / 1000);
    scroller.scrollTop = y;
    const [a, b] = visibleRange();
    let empty = 0;
    for (let i = a; i <= b; i++) if (pages[i].state !== "done" && !pages[i].canvas) empty++;
    blank.push(empty);
    if (y >= target) break;
  }
  po.disconnect();
  const dur = prev - t0;
  const ft = frames.slice(1);
  const canvases = pages.filter((p) => p.canvas).length;
  // Which page and phase was running during each long task (> 100 ms)?
  const blame = longAt.filter(([, d]) => d > 100).map(([s, d]) => {
    const hit = phases.find((ph) => ["canvas", "text"].some((k) => ph[k][0] <= s + d && s <= ph[k][1]));
    const phase = hit ? (hit.text[0] <= s + d && s <= hit.text[1] && !(hit.canvas[0] <= s + d && s <= hit.canvas[1]) ? "text" : "canvas") : "unknown";
    return { ms: Math.round(d), page: hit?.page ?? null, phase };
  });
  const slowest = [...phases].sort((a, b) => (b.canvas[1] - b.canvas[0]) - (a.canvas[1] - a.canvas[0])).slice(0, 3)
    .map((ph) => ({ page: ph.page, canvas_ms: Math.round(ph.canvas[1] - ph.canvas[0]), text_ms: Math.round(ph.text[1] - ph.text[0]), spans: ph.spans }));
  return { long_task_blame: blame, slowest_pages: slowest, speed_px_s: speed, pages_scrolled: lastPage + 1, duration_s: +(dur / 1000).toFixed(2), frames: ft.length,
    fps: +(ft.length / (dur / 1000)).toFixed(1), frame_ms_median: q(ft, 0.5), frame_ms_p95: q(ft, 0.95), frame_ms_p99: q(ft, 0.99),
    frames_over_16_7ms_pct: +((100 * ft.filter((x) => x > 16.7 * 1.25).length) / ft.length).toFixed(2),
    frames_over_33ms_pct: +((100 * ft.filter((x) => x > 33.4).length) / ft.length).toFixed(2),
    frames_with_blank_page_pct: +((100 * blank.filter((x) => x > 0).length) / blank.length).toFixed(2),
    long_tasks: longTasks.length, long_task_ms_max: longTasks.length ? Math.max(...longTasks) : 0,
    live_canvases_at_end: canvases, js_heap_mb: performance.memory ? +(performance.memory.usedJSHeapSize / 1048576).toFixed(1) : null };
}

async function benchJump(path, n = 10) {
  await open(path);
  await sleep(500);
  const lat = [];
  for (let k = 0; k < n; k++) {
    const i = Math.floor(((k * 7919) % 97) / 97 * pages.length);
    const t = performance.now();
    scroller.scrollTop = tops[i];
    while (pages[i].state !== "done") await raf();
    lat.push(performance.now() - t);
    await sleep(150);
  }
  return { jumps: n, jump_to_rendered_ms_median: q(lat, 0.5), jump_to_rendered_ms_p95: q(lat, 0.95), jump_ms_max: +Math.max(...lat).toFixed(1) };
}

// Hit-testing of the text layer: the point at the centre of a word must land in that word,
// and a word selection from there must return the word (what double-click does).
async function selectionCheck() {
  const layer = pages[0]?.text;
  if (!layer) return null;
  // Any script: Latin words, and Persian words in the RTL fixture.
  const WORD = /\p{L}{4,}/u;
  const spans = [...layer.querySelectorAll("span")].filter((s) => s.firstChild?.nodeType === Node.TEXT_NODE && WORD.test(s.firstChild.textContent));
  const picks = spans.filter((_, k) => k % Math.max(1, Math.floor(spans.length / 12)) === 0).slice(0, 12);
  let ok = 0;
  const fails = [];
  for (const s of picks) {
    const node = s.firstChild;
    const m = WORD.exec(node.textContent);
    const r = document.createRange();
    r.setStart(node, m.index); r.setEnd(node, m.index + m[0].length);
    const box = r.getBoundingClientRect();
    s.scrollIntoView({ block: "center" });
    await raf();
    const b2 = r.getBoundingClientRect();
    const pos = document.caretPositionFromPoint(b2.left + b2.width / 2, b2.top + b2.height / 2);
    const sel = getSelection();
    if (pos?.offsetNode) {
      sel.collapse(pos.offsetNode, pos.offset);
      sel.modify("move", "backward", "word");
      sel.modify("extend", "forward", "word");
    }
    const got = sel.toString().trim();
    if (got.includes(m[0])) ok++; else fails.push([m[0], got]);
    sel.removeAllRanges();
    void box;
  }
  return { words: picks.length, picked_correctly: ok, fails: fails.slice(0, 5) };
}

// Real-input check: report screen targets (CSS px in the viewport), then wait for an outside
// script to send a genuine double-click on the word and a drag across the line.
async function benchSelect(path) {
  await open(path);
  while (pages[0].state !== "done") await raf();
  await sleep(300);
  const WORD = /\p{L}{4,}/u;
  const vis = (r) => r.top > 60 && r.bottom < scroller.clientHeight - 20 && r.left > 0 && r.right < scroller.clientWidth;
  const spans = [...pages[0].text.querySelectorAll("span")]
    .filter((s) => s.firstChild?.nodeType === Node.TEXT_NODE && vis(s.getBoundingClientRect()));
  // Prefer a right-to-left word when the page has any (the Persian fixture).
  const PERSIAN = /[؀-ۿﭐ-﷿ﹰ-﻿]{2,}/u;
  const rtlSpans = spans.filter((s) => PERSIAN.test(s.firstChild.textContent));
  const W = rtlSpans.length ? PERSIAN : WORD;
  const wordSpan = (rtlSpans.length ? rtlSpans : spans.filter((s) => WORD.test(s.firstChild.textContent)))[2] ?? spans[0];
  const node = wordSpan.firstChild;
  const m = W.exec(node.textContent);
  const wr = document.createRange();
  wr.setStart(node, m.index); wr.setEnd(node, m.index + m[0].length);
  const wb = wr.getBoundingClientRect();
  // A visual line = all text-layer spans sharing a baseline (PDF.js splits RTL text into
  // one span per word). Take the line with the most words that does not hold the word target.
  const byLine = new Map();
  for (const s of spans) {
    if (s === wordSpan || !s.textContent.trim()) continue;
    const k = Math.round(s.getBoundingClientRect().bottom / 3);
    byLine.set(k, [...(byLine.get(k) ?? []), s]);
  }
  const lineSpans = [...byLine.values()].sort((a, b) =>
    b.reduce((n, s) => n + s.textContent.trim().split(/\s+/).length, 0) - a.reduce((n, s) => n + s.textContent.trim().split(/\s+/).length, 0))[0]
    .sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left);
  const edge = (s, atEnd) => {
    const t = s.firstChild, r = document.createRange();
    r.setStart(t, atEnd ? t.length - 1 : 0); r.setEnd(t, atEnd ? t.length : 1);
    return r.getBoundingClientRect();
  };
  const leftSpan = lineSpans[0], rightSpan = lineSpans.at(-1);
  const lineRtl = /[؀-ۿ]/.test(lineSpans.map((s) => s.textContent).join(""));
  // Visual left/right edges of the line, whatever the text direction.
  const lr = leftSpan.getBoundingClientRect(), rr = rightSpan.getBoundingClientRect();
  const fb = lineRtl ? { left: lr.left, top: lr.top, height: lr.height } : edge(leftSpan, false);
  const lb = lineRtl ? { right: rr.right, top: rr.top, height: rr.height } : edge(rightSpan, true);
  const pieces = lineSpans.map((s) => s.textContent.trim()).filter(Boolean);
  const lt = { textContent: pieces.join(" ") };
  const res = { word_expected: m[0], line_expected: lt.textContent, line_pieces: pieces.length, events: [] };
  let dbl = false;
  document.addEventListener("dblclick", (e) => { dbl = true; setTimeout(() => {
    res.double_click = { trusted: e.isTrusted, selected: getSelection().toString() };
  }, 150); });
  document.addEventListener("mouseup", (e) => {
    res.events.push({ type: "mouseup", trusted: e.isTrusted, detail: e.detail });
    if (dbl && e.detail === 1) setTimeout(() => { res.drag = { trusted: e.isTrusted, selected: getSelection().toString() }; }, 150);
  });
  // Right-to-left lines start at the right edge, as a reader would drag.
  const start = lineRtl ? [lb.right - 1, lb.top + lb.height / 2] : [fb.left + 1, fb.top + fb.height / 2];
  const end = lineRtl ? [fb.left + 1, fb.top + fb.height / 2] : [lb.right - 1, lb.top + lb.height / 2];
  await invoke("note", { value: { dpr: dpr(), word: [wb.left + wb.width / 2, wb.top + wb.height / 2], drag_start: start, drag_end: end } });
  const until = performance.now() + 30000;
  while (!(res.double_click && res.drag) && performance.now() < until) await sleep(100);
  res.word_ok = !!res.double_click?.selected.trim().includes(m[0]);
  res.word_span_text = wordSpan.textContent;
  res.word_span_chars = wordSpan.textContent.length;
  const norm = (s) => s.replace(/\s+/g, " ").trim();
  // Every piece of the line must be inside the selection (order is the browser's business).
  const sel = res.drag ? norm(res.drag.selected) : "";
  res.line_pieces_selected = pieces.filter((p) => sel.includes(norm(p))).length;
  res.line_ok = !!res.drag && res.line_pieces_selected === pieces.length;
  return res;
}

// ---- start -------------------------------------------------------------------------------
const cfg = await invoke("bench_config");
const files = await invoke("list_fixtures");
for (const f of files) $("file").append(new Option(f.split(/[\\/]/).pop(), f));
$("file").addEventListener("change", (e) => open(e.target.value).then((r) => r && status(`${r.info.pages.length} pages · first page ${(r.t.canvas - r.t.start).toFixed(0)} ms`)));

if (cfg) {
  status(`bench: ${cfg.mode}`);
  await sleep(400); // let the window settle and focus
  let result;
  try {
    if (cfg.mode === "open") result = await benchOpen(cfg.pdf);
    else if (cfg.mode === "scroll") {
      const n = Number(cfg.pages ?? 100);
      result = { slow: await benchScroll(cfg.pdf, n, 1200), fast: await benchScroll(cfg.pdf, n, 3000) };
    } else if (cfg.mode === "jump") result = await benchJump(cfg.pdf);
    else if (cfg.mode === "select") result = await benchSelect(cfg.pdf);
    result = { mode: cfg.mode, pdf: cfg.pdf.split(/[\\/]/).pop(), dpr: dpr(), viewport: [scroller.clientWidth, scroller.clientHeight], scale: +scale.toFixed(3), ...result };
  } catch (e) {
    result = { mode: cfg.mode, error: String(e?.stack ?? e) };
  }
  await invoke("report", { result });
} else if (files.length) {
  $("file").value = files[0];
  $("file").dispatchEvent(new Event("change"));
}
