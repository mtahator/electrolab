"""Summarise spike S5 outputs into out/report.md."""
import json
from pathlib import Path

out = Path(__file__).parent / "out"
L = []
p = L.append
p("## Layout (9 fixture papers, first ≤ 20 pages each)\n")
p("| model | threads | model ms/page (median of papers) | 20-page paper total s (min–max) | captions found | numbered equations found |")
p("|---|---|---|---|---|---|")
for f in sorted(out.glob("layout-*.json")):
    r = json.loads(f.read_text("utf-8"))
    ps = r["papers"]
    full = [x["total_s"] for x in ps if x["pages"] == 20]
    med = sorted(x["model_ms_median"] for x in ps)[len(ps) // 2]
    p(f"| {r['model']} | {r['threads']} | {med} | {min(full)}–{max(full)} | {sum(x['captions_found'] for x in ps)}/{sum(x['captions'] for x in ps)} | "
      f"{sum(x['equations_found'] for x in ps)}/{sum(x['equation_numbers'] for x in ps)} |")

p("\n## OCR of image-only pages (rendered at 200 dpi, PDF text layer not used)\n")
p("| engine | pages | per page s (detect + read) | word agreement with reference |")
p("|---|---|---|---|")
for name, label in (("ocr-en-naca.json", "PP-OCRv5 mobile, English (4 threads)"), ("ocr-fa-wiki.json", "PP-OCRv5 mobile, Arabic script, Persian page (lines reversed)")):
    f = out / name
    if f.exists():
        r = json.loads(f.read_text("utf-8"))
        rows = r["pages"]
        key = "f1_lines_reversed" if "f1_lines_reversed" in rows[0] else "f1"
        p(f"| {label} | {r['pdf']} p{rows[0]['page']}–{rows[-1]['page']} | " +
          ", ".join(f"{(x['detect_ms'] + x['recognise_ms']) / 1000:.1f}" for x in rows) + " | " + ", ".join(str(x[key]) for x in rows) + " |")
f = out / "winocr-naca.jsonl"
if f.exists():
    import sys
    sys.path.insert(0, str(Path(__file__).parent))
    from ocr import f1
    ref = (Path(__file__).parents[1] / "s2-pdfium" / "out" / "naca-report-1135.pdfium.txt").read_text("utf-8").split("\f")
    rows = [json.loads(l) for l in open(f, encoding="utf-8-sig") if l.strip().startswith("{")]
    p("| Windows built-in OCR (Windows.Media.Ocr), English | naca-report-1135 p2–p6 | " + ", ".join(f"{x['ms'] / 1000:.2f}" for x in rows) +
      " | " + ", ".join(str(f1(x["text"], ref[1 + k])) for k, x in enumerate(rows)) + " |")

f = out / "formulas.json"
if f.exists():
    r = json.loads(f.read_text("utf-8"))
    ms = sorted(x["ms"] for x in r["formulas"])
    multi = [x["ms"] for x in r["formulas"] if x["latex"].startswith("\\begin{array}")]
    single = [x["ms"] for x in r["formulas"] if not x["latex"].startswith("\\begin{array}")]
    p("\n## Formula → LaTeX (LaTeX-OCR / pix2tex ONNX via rapid_latex_ocr 0.0.9, CPU)\n")
    p(f"{len(ms)} formulas from 2 papers; model load {r['load_s']} s (first run includes download). "
      f"Single-line: {len(single)} formulas, {min(single)}–{max(single)} ms (median {sorted(single)[len(single) // 2]}). "
      f"Multi-line blocks: {len(multi)} formulas, {min(multi)}–{max(multi)} ms.")
text = "\n".join(L)
(out / "report.md").write_text(text + "\n", "utf-8", newline="\n")
print(text)
