// Fit an invisible word to the printed one: the printed font is not available to the page, so a
// plain font is stretched to the word's printed width (design language §9.10).

/** The font of the text layer; the same as `--font-fit` in tokens.css. */
const FONT = "sans-serif";
const AT = 100;
const widths = new Map<string, number>();
let ctx: CanvasRenderingContext2D | null = null;

/** Natural width of `text` at 1 px font size. */
function natural(text: string): number {
  let w = widths.get(text);
  if (w === undefined) {
    ctx ??= document.createElement("canvas").getContext("2d");
    if (!ctx) return 0;
    ctx.font = `${AT}px ${FONT}`;
    w = ctx.measureText(text).width / AT;
    if (widths.size > 50_000) widths.clear();
    widths.set(text, w);
  }
  return w;
}

/** Horizontal scale that makes `text` at `fontSize` as wide as `width`. */
export function fitX(text: string, fontSize: number, width: number): number {
  const w = natural(text) * fontSize;
  return w > 0 && width > 0 ? width / w : 1;
}
