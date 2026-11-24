// The container owns the width (design language §5.4): outside the kit, only the surfaces listed
// here may set a fixed width; nothing may set a minimum width or use viewport widths.

/** Surfaces of the Reader that own their width (they float or are columns of the frame). */
export const SURFACES = new Set([".tool-flyout", ".find-bar-input", ".library-nav", ".palette", ".table-view"]);
/** Fixed widths up to this are parts of a control, not layout (px). */
const SMALL = 180;

/** Problems in a style sheet, as `line: declaration`. */
export function findWidths(css, surfaces = SURFACES) {
  const lines = css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " ")).split("\n");
  const found = [];
  let selector = "";
  lines.forEach((line, i) => {
    const open = line.match(/^\s*([^{}@]+)\{/);
    if (open) selector = open[1].trim();
    const d = line.match(/^\s*(min-width|width|inline-size|min-inline-size)\s*:\s*([^;]+);?/);
    if (!d || surfaces.has(selector)) return;
    const [, prop, value] = d;
    const px = Math.max(0, ...[...value.matchAll(/(\d+(?:\.\d+)?)px/g)].map((m) => Number(m[1])));
    const wide = /\d+vw\b/.test(value) || px > SMALL;
    if (wide) found.push(`${i + 1}: ${selector} { ${prop}: ${value.trim()} }`);
  });
  return found;
}
