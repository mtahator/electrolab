// Tokens only, and no widths on content (guideline §15, P1.MD.KIT.4): no raw colours or font families in UI style sheets.
// Colours and fonts live in ui/kit/src/tokens.css; everything else refers to them with var(--…).
//
//   node ui/kit/scripts/check-tokens.mjs     (exit 1 and a list when something is raw)

import { readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { findWidths } from "./widths.mjs";

const ui = resolve(import.meta.dirname, "../..");
const allowed = new Set([resolve(ui, "kit/src/tokens.css")]);
const found = [];
const wide = [];
/** The kit's own sheets define the surfaces. */
const kit = resolve(ui, "kit/src");

function walk(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.name === "node_modules" || e.name === "dist" || e.name === "public") continue;
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (e.name.endsWith(".css") && !allowed.has(p)) check(p);
  }
}

function check(file) {
  if (!file.startsWith(kit)) for (const w of findWidths(readFileSync(file, "utf8"))) wide.push(`${relative(ui, file)}:${w}`);
  const lines = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " ")).split("\n");
  lines.forEach((line, i) => {
    const raw =
      /#[0-9a-f]{3,8}\b/i.test(line) ||
      /\b(rgb|rgba|hsl|hsla|hwb|lab|lch|oklch)\(/i.test(line) ||
      (/font-family\s*:/.test(line) && !/var\(--font-/.test(line) && !/inherit/.test(line));
    if (raw) found.push(`${relative(ui, file)}:${i + 1}: ${line.trim()}`);
  });
}

walk(ui);
if (wide.length > 0) {
  console.error("Widths on content (the container owns the width, design language §5.4; surfaces are listed in widths.mjs):");
  for (const w of wide) console.error(`  ${w}`);
  process.exitCode = 1;
}
if (found.length > 0) {
  console.error("Raw colours or fonts outside ui/kit/src/tokens.css (use a token):");
  for (const f of found) console.error(`  ${f}`);
  process.exit(1);
}
if (wide.length === 0) console.log("check-tokens: ok");
