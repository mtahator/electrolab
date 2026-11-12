// Third parser: PDF.js (what a browser-based viewer sees). Lists page-1 annotations.
// usage: node verify_pdfjs.mjs <annotated.pdf> <out.json>   (uses pdfjs-dist from spike S1)
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const require = createRequire(new URL("../s1-tauri-pdfjs/package.json", import.meta.url));
const { getDocument } = await import(pathToFileURL(require.resolve("pdfjs-dist/legacy/build/pdf.mjs")).href);
const [file, out] = process.argv.slice(2);
const task = getDocument({ data: new Uint8Array(await readFile(file)), isEvalSupported: false, verbosity: 0 });
const doc = await task.promise;
const page = await doc.getPage(1);
const anns = (await page.getAnnotations()).filter((a) => a.subtype !== "Link");
const rows = anns.map((a) => ({
  subtype: a.subtype, id: a.id, nm: a.annotationName ?? null, author: a.titleObj?.str ?? null,
  contents: a.contentsObj?.str ?? null, hasAppearance: a.hasAppearance, inReplyTo: a.inReplyTo ?? null,
  replyType: a.replyType ?? null, quadPoints: a.quadPoints ? a.quadPoints.length / 8 : null,
  lineCoordinates: a.lineCoordinates ?? null, popupRef: a.popupRef ?? null,
}));
await writeFile(out, JSON.stringify(rows, null, 1));
for (const r of rows) console.log(`${r.subtype.padEnd(10)} nm=${String(r.nm).padEnd(16)} AP=${String(r.hasAppearance).padEnd(5)} replyTo=${String(r.inReplyTo).padEnd(6)} quads=${r.quadPoints} author=${r.author} contents=${String(r.contents ?? "").slice(0, 50)}`);
await task.destroy();
