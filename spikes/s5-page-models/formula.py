"""Spike S5 — formula image → LaTeX on CPU (LaTeX-OCR / pix2tex via rapid_latex_ocr, ONNX).

Formulas are found by a layout model (spike layout.py), cropped from a 4× PDFium render, and
converted. Writes each crop as PNG next to out/formulas.json so image and LaTeX can be compared.
usage: python -I -X utf8 formula.py <layout-model> <out-dir> <max-formulas> <pdf> <first-page> <n-pages> [<pdf> <first> <n>]...
"""
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import pypdfium2 as pdfium  # noqa: E402
from layout import Model  # noqa: E402

from rapid_latex_ocr import LaTeXOCR  # noqa: E402


def main():
    model_name, out, limit = sys.argv[1], Path(sys.argv[2]), int(sys.argv[3])
    specs = sys.argv[4:]
    (out / "formulas").mkdir(parents=True, exist_ok=True)
    layout = Model(model_name, 8)
    t = time.perf_counter()
    ocr = LaTeXOCR()
    load_s = time.perf_counter() - t
    rows = []
    for k in range(0, len(specs), 3):
        pdf, first, n = specs[k], int(specs[k + 1]), int(specs[k + 2])
        doc = pdfium.PdfDocument(pdf)
        for i in range(first - 1, first - 1 + n):
            page = doc[i]
            boxes = [b for b in layout(page.render(scale=2.0).to_pil().convert("RGB")) if b[0] == "formula"]
            big = page.render(scale=4.0).to_pil().convert("RGB")
            for label, score, x0, y0, x1, y1 in boxes:
                if len(rows) >= limit:
                    break
                pad = 6
                img = big.crop((max(0, 2 * x0 - pad), max(0, 2 * y0 - pad), 2 * x1 + pad, 2 * y1 + pad))
                name = f"{Path(pdf).stem}-p{i + 1}-{len(rows):02d}.png"
                img.save(out / "formulas" / name)
                t0 = time.perf_counter()
                res = ocr(str(out / "formulas" / name))
                ms = round(1000 * (time.perf_counter() - t0))
                latex = res[0] if isinstance(res, tuple) else res
                rows.append({"image": name, "ms": ms, "latex": latex, "w": img.width, "h": img.height})
                print(f"{ms:5} ms  {name}  {latex[:110]}", flush=True)
    (out / "formulas.json").write_text(json.dumps({"load_s": round(load_s, 2), "formulas": rows}, indent=1, ensure_ascii=False), "utf-8")


if __name__ == "__main__":
    main()
