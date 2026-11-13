"""Render pages to PNG (PDFium, 200 dpi), or score Windows OCR output against reference text.

  python -I -X utf8 winocr_score.py render <pdf> <first> <n> <out-dir>
  python -I -X utf8 winocr_score.py score  <winocr.jsonl> <reference.txt> <first>
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

if sys.argv[1] == "render":
    import pypdfium2 as pdfium
    pdf, first, n, out = sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), Path(sys.argv[5])
    out.mkdir(parents=True, exist_ok=True)
    doc = pdfium.PdfDocument(pdf)
    for i in range(first - 1, first - 1 + n):
        doc[i].render(scale=200 / 72).to_pil().convert("RGB").save(out / f"page-{i + 1}.png")
        print(out / f"page-{i + 1}.png")
else:
    from ocr import f1
    rows = [json.loads(l) for l in open(sys.argv[2], encoding="utf-8-sig") if l.strip().startswith("{")]
    ref = Path(sys.argv[3]).read_text("utf-8").split("\f")
    first = int(sys.argv[4])
    for k, r in enumerate(rows):
        print(json.dumps({"image": r["image"], "ms": r["ms"], "lines": r["lines"], "f1": f1(r["text"], ref[first - 1 + k])}))
