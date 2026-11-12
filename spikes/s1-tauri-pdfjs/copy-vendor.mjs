// Copy PDF.js into ui/vendor so the app loads nothing from the network.
import { cpSync, mkdirSync } from "node:fs";

const src = "node_modules/pdfjs-dist";
const dst = "ui/vendor/pdfjs";
mkdirSync(dst, { recursive: true });
for (const f of ["build/pdf.mjs", "build/pdf.worker.mjs", "web/pdf_viewer.css"]) {
  cpSync(`${src}/${f}`, `${dst}/${f.split("/").pop()}`);
}
cpSync(`${src}/cmaps`, `${dst}/cmaps`, { recursive: true });
for (const d of ["standard_fonts", "wasm", "iccs"]) cpSync(`${src}/${d}`, `${dst}/${d}`, { recursive: true });
console.log("vendored pdfjs-dist into", dst);
