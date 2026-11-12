"""Compare PDFium text with the PyMuPDF reference, per paper.

No ground truth exists, so the comparison has two parts:
  * agreement: word-multiset F1 and in-order similarity between the two extractors;
  * defect signals in each extractor alone (things that are wrong whatever the truth is):
    replacement chars, private-use glyphs, unexpanded ligatures, merged words, split words.

usage: python -I compare_text.py <out-dir>   (reads *.pdfium.txt and *.pymupdf.txt)
"""
import collections
import difflib
import json
import re
import sys
import unicodedata
from pathlib import Path

WORD = re.compile(r"\S+")
LIGATURES = re.compile("[ﬀ-ﬆ]")


def norm(text: str) -> str:
    text = unicodedata.normalize("NFKC", text)
    text = re.sub(r"(\w)-\s*\n\s*(\w)", r"\1\2", text)  # line-end hyphenation
    return text


def words(text: str) -> list[str]:
    return WORD.findall(text)


def f1(a: list[str], b: list[str]) -> float:
    ca, cb = collections.Counter(a), collections.Counter(b)
    common = sum((ca & cb).values())
    if not a or not b:
        return 0.0
    p, r = common / len(a), common / len(b)
    return 0.0 if common == 0 else round(2 * p * r / (p + r), 4)


def in_order(a_pages: list[str], b_pages: list[str]) -> float:
    ratios = []
    for a, b in zip(a_pages, b_pages):
        wa, wb = words(norm(a)), words(norm(b))
        if wa or wb:
            ratios.append(difflib.SequenceMatcher(None, wa, wb, autojunk=False).ratio())
    return round(sum(ratios) / len(ratios), 4) if ratios else 0.0


def defects(text: str) -> dict:
    ws = words(norm(text))
    alpha = [w for w in ws if w.isalpha()]
    return {
        "words": len(ws),
        "replacement": text.count("�"),
        "private_use": sum(1 for c in text if "" <= c <= ""),
        "ligatures": len(LIGATURES.findall(text)),
        # merged words: long alphabetic runs that are almost never real words
        "merged_words": sum(1 for w in alpha if len(w) >= 25),
        # split words: share of single-letter alphabetic tokens other than a/A/I
        "single_letters_pct": round(100 * sum(1 for w in alpha if len(w) == 1 and w not in "aAI")
                                    / max(len(alpha), 1), 2),
    }


def sample_diff(a: str, b: str, n: int = 8) -> list[str]:
    wa, wb = words(norm(a)), words(norm(b))
    out = []
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, wa, wb, autojunk=False).get_opcodes():
        if tag != "equal":
            out.append(f"{tag}: pdfium {' '.join(wa[i1:i2])[:80]!r} | pymupdf {' '.join(wb[j1:j2])[:80]!r}")
            if len(out) >= n:
                break
    return out


def main() -> None:
    out = Path(sys.argv[1])
    rows = []
    for pf in sorted(out.glob("*.pdfium.txt")):
        stem = pf.name[: -len(".pdfium.txt")]
        mf = out / f"{stem}.pymupdf.txt"
        if not mf.exists():
            continue
        a, b = pf.read_text("utf-8"), mf.read_text("utf-8")
        a_pages, b_pages = a.split("\f"), b.split("\f")
        rows.append({
            "file": stem,
            "f1_raw": f1(words(a), words(b)),
            "f1_normalised": f1(words(norm(a)), words(norm(b))),
            "in_order": in_order(a_pages, b_pages[: len(a_pages)]),
            "pdfium": defects(a),
            "pymupdf": defects(b),
            "diff_sample_page1": sample_diff(a_pages[0], b_pages[0]),
        })
    (out / "compare_text.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False), "utf-8")
    for r in rows:
        print(json.dumps({k: r[k] for k in ("file", "f1_raw", "f1_normalised", "in_order", "pdfium", "pymupdf")},
                         ensure_ascii=False))


if __name__ == "__main__":
    main()
