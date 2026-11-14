// Tooltips (guideline §6, §8): any element with `data-tip` shows it after a short delay on hover
// or keyboard focus. One tooltip element per window; installed once at start-up.

const DELAY_MS = 500;

export function installTooltips(): void {
  const tip = document.createElement("div");
  tip.className = "ep-tooltip";
  tip.setAttribute("role", "tooltip");
  tip.hidden = true;
  document.body.append(tip);
  let timer: number | undefined;
  let current: HTMLElement | null = null;

  const hide = (): void => {
    window.clearTimeout(timer);
    tip.hidden = true;
    current = null;
  };
  const show = (el: HTMLElement): void => {
    const text = el.dataset.tip;
    if (!text || !el.isConnected) return;
    tip.textContent = text;
    tip.hidden = false;
    const r = el.getBoundingClientRect();
    const w = tip.offsetWidth;
    const h = tip.offsetHeight;
    // Below by default; to the right for vertical rails; above near the bottom edge.
    let x = r.left + r.width / 2 - w / 2;
    let y = r.bottom + 6;
    if (el.closest("[data-tip-side='right']")) {
      x = r.right + 8;
      y = r.top + r.height / 2 - h / 2;
    } else if (y + h > window.innerHeight - 4) {
      y = r.top - h - 6;
    }
    tip.style.left = `${Math.max(4, Math.min(x, window.innerWidth - w - 4))}px`;
    tip.style.top = `${Math.max(4, y)}px`;
  };
  const arm = (e: Event): void => {
    const el = (e.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-tip]") ?? null;
    if (el === current) return;
    hide();
    if (!el) return;
    current = el;
    timer = window.setTimeout(() => show(el), DELAY_MS);
  };
  document.addEventListener("pointerover", arm);
  document.addEventListener("focusin", arm);
  document.addEventListener("pointerdown", hide, true);
  document.addEventListener("keydown", hide, true);
  document.addEventListener("scroll", hide, true);
  document.addEventListener("focusout", hide);
}
