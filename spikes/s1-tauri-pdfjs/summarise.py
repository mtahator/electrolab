"""Summarise spike S1 measurements in out/*.json as Markdown tables (printed and written to out/report.md)."""
import json
from pathlib import Path

out = Path(__file__).parent / "out"
lines = []
p = lines.append


def load(prefix):
    for f in sorted(out.glob(f"{prefix}-*.json")):
        if f.stem.endswith(".targets"):
            continue
        yield f.stem[len(prefix) + 1:], json.loads(f.read_text("utf-8"))


p("## Open → first page (budget < 300 ms)\n")
p("| paper | pages | cold canvas ms | warm canvas ms (median) | warm text layer ms (median) | warm max ms | bytes read | requests | word pick |")
p("|---|---|---|---|---|---|---|---|---|")
for name, r in load("open"):
    if "error" in r:
        p(f"| {name} | error: {r['error'][:80]} |||||||| ")
        continue
    warm_max = max(w["first_text_layer_ms"] for w in r["warm"])
    s = r.get("selection") or {}
    p(f"| {name} | {r['pages']} | {r['cold']['first_canvas_ms']} | {r['warm_first_canvas_ms_median']} | "
      f"{r['warm_first_text_layer_ms_median']} | {warm_max} | {r['cold_bytes_fetched'] // 1024} KB "
      f"({100 * r['cold_bytes_share']:.1f}%) | {r['cold_requests']} | {s.get('picked_correctly')}/{s.get('words')} |")

p("\n## Continuous scroll (budget: frame < 16 ms → 60 fps)\n")
p("| paper | speed px/s | pages | fps | frame ms median / p95 / p99 | frames > 21 ms | frames > 33 ms | frames showing a blank page | long tasks (max ms) | live canvases | JS heap MB |")
p("|---|---|---|---|---|---|---|---|---|---|---|")
for name, r in load("scroll"):
    if "error" in r:
        p(f"| {name} | error: {r['error'][:80]} |||||||||| ")
        continue
    for k in ("slow", "fast"):
        s = r[k]
        p(f"| {name} | {s['speed_px_s']} | {s['pages_scrolled']} | {s['fps']} | {s['frame_ms_median']} / {s['frame_ms_p95']} / "
          f"{s['frame_ms_p99']} | {s['frames_over_16_7ms_pct']}% | {s['frames_over_33ms_pct']}% | {s['frames_with_blank_page_pct']}% | "
          f"{s['long_tasks']} ({round(s['long_task_ms_max'])}) | {s['live_canvases_at_end']} | {s['js_heap_mb']} |")

p("\n## Jump to a page → rendered\n")
p("| paper | jumps | median ms | p95 ms | max ms |")
p("|---|---|---|---|---|")
for name, r in load("jump"):
    if "error" in r:
        p(f"| {name} | error: {r['error'][:80]} |||| ")
        continue
    p(f"| {name} | {r['jumps']} | {r['jump_to_rendered_ms_median']} | {r['jump_to_rendered_ms_p95']} | {r['jump_ms_max']} |")

p("\n## Real mouse input (genuine Windows mouse events, `real-input.ps1`)\n")
p("| paper | word | double-click selected | trusted | word ok | line pieces selected | line ok |")
p("|---|---|---|---|---|---|---|")
for name, r in load("select"):
    if "error" in r:
        p(f"| {name} | error: {r['error'][:80]} |||||| ")
        continue
    dc = r.get("double_click") or {}
    p(f"| {name} | {r['word_expected']} | {dc.get('selected', '').strip()} | {dc.get('trusted')} | {r['word_ok']} | "
      f"{r.get('line_pieces_selected')}/{r.get('line_pieces')} | {r['line_ok']} |")

any_r = next(iter(load("open")), (None, {}))[1]
p(f"\nDisplay: {any_r.get('idle_fps')} Hz, device pixel ratio {any_r.get('dpr')}, viewport {any_r.get('viewport')}, "
  f"page scale {any_r.get('scale')} (fit width, capped at 2).")
text = "\n".join(lines)
(out / "report.md").write_text(text + "\n", "utf-8", newline="\n")
print(text)
