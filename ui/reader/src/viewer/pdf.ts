// PDF.js setup: everything it loads comes from the app itself (copied by scripts/copy-pdfjs.mjs),
// scripting is off, and one document proxy is shared per open paper.

import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { blobUrl } from "../host";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

// `isEvalSupported: false`: the app's CSP forbids eval, so glyphs are drawn without it.
const DOC_OPTIONS = {
  disableAutoFetch: true,
  rangeChunkSize: 262144,
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  wasmUrl: "/pdfjs/wasm/",
  iccUrl: "/pdfjs/iccs/",
  isEvalSupported: false,
  enableXfa: false,
} as const;

interface Entry {
  task: pdfjs.PDFDocumentLoadingTask;
  users: number;
}

const docs = new Map<string, Entry>();

/** Open (or share) the PDF.js document for a stored file. Call `release` when done. */
export function acquire(hash: string): Promise<pdfjs.PDFDocumentProxy> {
  let e = docs.get(hash);
  if (!e) {
    e = { task: pdfjs.getDocument({ url: blobUrl(hash), ...DOC_OPTIONS }), users: 0 };
    docs.set(hash, e);
  }
  e.users += 1;
  return e.task.promise;
}

/** Stop using a document; the last user closes it. */
export function release(hash: string): void {
  const e = docs.get(hash);
  if (!e) return;
  e.users -= 1;
  if (e.users <= 0) {
    docs.delete(hash);
    void e.task.destroy();
  }
}

export { pdfjs };
export type PDFDocumentProxy = pdfjs.PDFDocumentProxy;
export type RenderTask = pdfjs.RenderTask;
