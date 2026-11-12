// Spike P0.S2 — do PDFium and PDF.js put text in the same place?
//
// For every horizontal PDF.js text item, find the PDFium character with the same
// glyph nearest to the item's start origin (e, f) and to its end (e + width, f).
// Both live in PDF user space (points, y up), so no conversion is needed.
// Target (technical-structure.md §7.4): agreement within 1 pt.
//
// usage: node compare_geometry.mjs <out-dir> <pdf>...
import { readFile, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";

const [outDir, ...pdfs] = process.argv.slice(2);
const nfkc = (s) => s.normalize("NFKC");
const quantile = (xs, q) => {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  return +s[Math.min(s.length - 1, Math.floor(q * s.length))].toFixed(3);
};
const share = (xs, lim) => (xs.length ? +((100 * xs.filter((x) => x <= lim).length) / xs.length).toFixed(2) : null);

function nearest(index, ch, x, y, useRight) {
  const list = index.get(ch);
  if (!list) return Infinity;
  let best = Infinity;
  for (const c of list) {
    // c = [u, ox, oy, l, b, r, t]
    const cx = useRight ? c[5] : c[1];
    const d = Math.hypot(cx - x, c[2] - y);
    if (d < best) best = d;
  }
  return best;
}

const rows = [];
for (const pdf of pdfs) {
  const stem = basename(pdf, extname(pdf));
  try {
    const pages = JSON.parse(await readFile(join(outDir, `${stem}.pdfium.chars.json`), "utf8"));
    const data = new Uint8Array(await readFile(pdf));
    const t0 = performance.now();
    const task = getDocument({ data, isEvalSupported: false, disableFontFace: true, verbosity: 0 });
    const doc = await task.promise;
    const start = [], end = [];
    let items = 0, skipped = 0, unmatched = 0, pagesDiffer = doc.numPages !== pages.length;
    for (let p = 1; p <= Math.min(doc.numPages, pages.length); p++) {
      const page = await doc.getPage(p);
      const content = await page.getTextContent();
      const index = new Map();
      for (const c of pages[p - 1].chars) {
        const k = nfkc(c[0]);
        for (const g of k) {
          if (!index.has(g)) index.set(g, []);
          index.get(g).push(c);
        }
      }
      for (const it of content.items) {
        const s = nfkc(it.str ?? "");
        if (!s.trim()) continue;
        items++;
        const [, b, c, , e, f] = it.transform;
        if (Math.abs(b) > 1e-6 || Math.abs(c) > 1e-6 || /^\s/.test(s) || /\s$/.test(s)) { skipped++; continue; }
        const ds = nearest(index, s[0], e, f, false);
        const de = nearest(index, s[s.length - 1], e + it.width, f, true);
        if (ds > 5) { unmatched++; continue; }
        start.push(ds);
        if (de <= 5) end.push(de);
      }
      page.cleanup();
    }
    const ms = performance.now() - t0;
    await task.destroy();
    rows.push({
      file: stem, items, skipped, unmatched,
      unmatched_pct: items ? +((100 * unmatched) / items).toFixed(2) : null,
      start_median_pt: quantile(start, 0.5), start_p95_pt: quantile(start, 0.95), start_within_1pt_pct: share(start, 1),
      end_median_pt: quantile(end, 0.5), end_p95_pt: quantile(end, 0.95), end_within_1pt_pct: share(end, 1),
      pdfjs_text_ms: +ms.toFixed(1), page_count_mismatch: pagesDiffer,
    });
  } catch (err) {
    rows.push({ file: stem, error: String(err) });
  }
  console.log(JSON.stringify(rows.at(-1)));
}
await writeFile(join(outDir, "compare_geometry.json"), JSON.stringify(rows, null, 1));
