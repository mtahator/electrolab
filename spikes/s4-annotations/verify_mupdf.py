"""Second, independent parser and renderer: MuPDF (via PyMuPDF; measurement only, never shipped).

Lists page-1 annotations with the properties other readers rely on, reports any MuPDF repair
warnings (a broken incremental update would trigger them), checks every mark is visible by
rendering with and without annotations, and writes a PNG for a human look.
usage: python -I -X utf8 verify_mupdf.py <annotated.pdf> <out-dir>
"""
import json
import sys
from pathlib import Path

import pymupdf

src, out = Path(sys.argv[1]), Path(sys.argv[2])
pymupdf.TOOLS.mupdf_warnings()            # clear
doc = pymupdf.open(src)
warnings = pymupdf.TOOLS.mupdf_warnings()
page = doc[0]
rows = []
for a in page.annots():
    x = a.xref
    rows.append({
        "type": a.type[1], "nm": a.info.get("name") or doc.xref_get_key(x, "NM")[1], "author": a.info.get("title"),
        "subject": a.info.get("subject"), "contents": a.info.get("content"),
        "has_AP": doc.xref_get_key(x, "AP")[0] != "null",
        "in_reply_to": doc.xref_get_key(x, "IRT")[1] if doc.xref_get_key(x, "IRT")[0] != "null" else None,
        "state": doc.xref_get_key(x, "State")[1] if doc.xref_get_key(x, "State")[0] != "null" else None,
        "line_ends": a.line_ends if a.type[1] == "Line" else None,
        "quads": len(a.vertices) // 4 if a.type[1] in ("Highlight", "Underline", "StrikeOut") and a.vertices else None,
    })

# visible? render with and without annotations
zoom = pymupdf.Matrix(2, 2)
on = page.get_pixmap(matrix=zoom, annots=True)
off = page.get_pixmap(matrix=zoom, annots=False)
for row, a in zip(rows, page.annots()):
    r = a.rect * zoom
    x0, y0, x1, y1 = (max(0, int(r.x0)), max(0, int(r.y0)), min(on.width, int(r.x1)), min(on.height, int(r.y1)))
    changed = 0
    for y in range(y0, y1):
        for xx in range(x0, x1):
            if any(abs(c1 - c2) > 24 for c1, c2 in zip(on.pixel(xx, y), off.pixel(xx, y))):
                changed += 1
    row["changed_pixels"] = changed
on.save(out / "mupdf-page1.png")

result = {"file": src.name, "mupdf_warnings": warnings, "page1_annotations": rows,
          "is_repaired": doc.is_repaired, "pages": doc.page_count}
(out / "verify-mupdf.json").write_text(json.dumps(result, indent=1, ensure_ascii=False), "utf-8")
print(json.dumps({"warnings": warnings, "is_repaired": doc.is_repaired}, ensure_ascii=False))
for r in rows:
    if r["nm"] and str(r["nm"]).startswith("ep-") or r["type"] == "Popup":
        print(f"{r['type']:10} nm={r['nm']:16} AP={r['has_AP']!s:5} IRT={r['in_reply_to']!s:8} state={r['state']!s:11} "
              f"ends={r['line_ends']} quads={r['quads']} changed={r['changed_pixels']} subj={r['subject']!r}")
