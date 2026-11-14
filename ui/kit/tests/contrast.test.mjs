// Contrast of every text and icon token pair in every theme (guideline §3.1, P1.MD.KIT.1):
// text ≥ 4.5 : 1, icons and large text ≥ 3 : 1.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/tokens.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** Variables of the block whose selector matches exactly. */
function block(selector) {
  const out = {};
  const re = /([^{}]+)\{([^{}]*)\}/g;
  for (const m of css.matchAll(re)) {
    if (m[1].trim() !== selector) continue;
    for (const d of m[2].matchAll(/--([\w-]+):\s*([^;]+);/g)) out[d[1]] = d[2].trim();
  }
  return out;
}

const light = block(":root");
const themes = {
  light,
  sepia: { ...light, ...block(':root[data-theme="sepia"]') },
  dark: { ...light, ...block(':root[data-theme="dark"]') },
  night: { ...light, ...block(':root[data-theme="night"]') },
  contrast: { ...light, ...block(':root[data-theme="contrast"]') },
};

function rgb(hex) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  assert.ok(m, `not a #rrggbb colour: ${hex}`);
  return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16) / 255);
}

function luminance(hex) {
  const [r, g, b] = rgb(hex).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a, b) {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

const TEXT = 4.5;
const ICON = 3;
const pairs = [
  ["fg", "bg", TEXT],
  ["fg", "bg-panel", TEXT],
  ["fg", "bg-sunken", TEXT],
  ["fg", "bg-hover", TEXT],
  ["fg-muted", "bg", TEXT],
  ["fg-muted", "bg-panel", TEXT],
  ["fg-muted", "bg-hover", ICON],
  ["accent", "bg", ICON],
  ["accent", "bg-panel", ICON],
  ["accent", "accent-soft", ICON],
  ["accent-fg", "accent", TEXT],
  ["danger", "bg", TEXT],
  ["danger", "bg-panel", TEXT],
  ["success", "bg-panel", TEXT],
  ["warning", "bg-panel", TEXT],
  ["border", "bg", 1.2],
];

for (const [name, t] of Object.entries(themes)) {
  test(`contrast in the ${name} theme`, () => {
    const low = [];
    for (const [fg, bg, min] of pairs) {
      const r = ratio(t[fg], t[bg]);
      if (r < min) low.push(`${fg} on ${bg}: ${r.toFixed(2)} < ${min}`);
    }
    assert.deepEqual(low, []);
  });
}
