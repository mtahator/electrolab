"""Merge spike outputs into one table (Markdown) plus aggregate numbers.

usage: python -I report.py <pdfium.jsonl> <pymupdf.jsonl> <out-dir> [stems...]
"""
import json
import statistics
import sys
from pathlib import Path


def load_jsonl(p):
    return {r["file"]: r for r in (json.loads(l) for l in open(p, encoding="utf-8-sig") if l.strip())}


def main():
    pdfium, mupdf, out = load_jsonl(sys.argv[1]), load_jsonl(sys.argv[2]), Path(sys.argv[3])
    only = set(sys.argv[4:])
    text = {r["file"]: r for r in json.loads((out / "compare_text.json").read_text("utf-8"))}
    geom = {r["file"]: r for r in json.loads((out / "compare_geometry.json").read_text("utf-8"))}
    rows = []
    print("| paper | pages | PDFium text ms/100p | PyMuPDF ms/100p | word F1 | in-order | "
          "PUA PDFium / PyMuPDF | U+FFFD PDFium / PyMuPDF | merged words PDFium / PyMuPDF | "
          "start ≤1pt % | start p95 pt | end ≤1pt % | unmatched % |")
    print("|" + "---|" * 13)
    for stem in sorted(pdfium):
        if only and stem not in only:
            continue
        a, b, t, g = pdfium[stem], mupdf.get(stem, {}), text.get(stem), geom.get(stem, {})
        if "error" in a or t is None:
            print(f"| {stem} | error | | | | | | | | | | | |")
            continue
        rows.append((a, b, t, g))
        print(f"| {stem} | {a['pages']} | {a['text_ms_per_100p']:.0f} | {b.get('text_ms_per_100p', 0):.0f} | "
              f"{t['f1_normalised']:.3f} | {t['in_order']:.3f} | "
              f"{t['pdfium']['private_use']} / {t['pymupdf']['private_use']} | "
              f"{t['pdfium']['replacement']} / {t['pymupdf']['replacement']} | "
              f"{t['pdfium']['merged_words']} / {t['pymupdf']['merged_words']} | "
              f"{g.get('start_within_1pt_pct')} | {g.get('start_p95_pt')} | {g.get('end_within_1pt_pct')} | "
              f"{g.get('unmatched_pct')} |")

    def med(xs):
        xs = [x for x in xs if x is not None]
        return round(statistics.median(xs), 3) if xs else None

    print()
    print("aggregate (median over papers):")
    print(" PDFium text ms/100p:", med([a["text_ms_per_100p"] for a, *_ in rows]),
          " max:", max(a["text_ms_per_100p"] for a, *_ in rows))
    print(" PDFium chars ms/100p:", med([a["chars_ms_per_100p"] for a, *_ in rows]),
          " max:", max(a["chars_ms_per_100p"] for a, *_ in rows))
    print(" PyMuPDF text ms/100p:", med([b.get("text_ms_per_100p") for _, b, _, _ in rows]))
    print(" papers over 2000 ms/100p (PDFium text):",
          [a["file"] for a, *_ in rows if a["text_ms_per_100p"] > 2000])
    print(" word F1 normalised:", med([t["f1_normalised"] for *_, t, _ in rows]),
          " min:", min(t["f1_normalised"] for *_, t, _ in rows))
    print(" in-order similarity:", med([t["in_order"] for *_, t, _ in rows]),
          " min:", min(t["in_order"] for *_, t, _ in rows))
    print(" start within 1 pt %:", med([g.get("start_within_1pt_pct") for *_, g in rows]),
          " min:", min((g.get("start_within_1pt_pct") or 0) for *_, g in rows))
    print(" end within 1 pt %:", med([g.get("end_within_1pt_pct") for *_, g in rows]))
    tot = lambda side, key: sum(t[side][key] for *_, t, _ in rows)
    for key in ("private_use", "replacement", "merged_words", "ligatures"):
        print(f" total {key}: PDFium {tot('pdfium', key)}  PyMuPDF {tot('pymupdf', key)}")


if __name__ == "__main__":
    main()
