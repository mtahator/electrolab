// Copy the data PDF.js needs at run time (character maps, standard fonts, decoders, colour
// profiles) into public/pdfjs, so the app never loads anything from the network.
import { cpSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const src = dirname(require.resolve("pdfjs-dist/package.json"));
const dst = join(import.meta.dirname, "..", "public", "pdfjs");
mkdirSync(dst, { recursive: true });
for (const d of ["cmaps", "standard_fonts", "wasm", "iccs"]) {
  cpSync(join(src, d), join(dst, d), { recursive: true });
}
console.log("pdfjs assets ->", dst);
