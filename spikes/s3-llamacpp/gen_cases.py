"""Build Explain test cases from the fixture papers' own abbreviation definitions.

A paper that writes "physics-informed neural network (PINN)" gives a case:
  term     = "PINN", selected at a later use (not the defining sentence),
  before/after = 500 characters around that use (what Explain sends to the model),
  expected = "physics-informed neural network" (the answer must contain it).
The check step gets the paper's passages about the term, definitions first (as ElectroChat does).

Text comes from the S2 spike's PDFium output (spikes/s2-pdfium/out/*.pdfium.txt).
usage: python -I -X utf8 gen_cases.py <s2-out-dir> <cases.json>
"""
import json
import re
import sys
from pathlib import Path

ABBR = re.compile(r"((?:[A-Za-z][\w\-]*\s+){1,6}?[A-Za-z][\w\-]*)\s*\(([A-Z][A-Za-z0-9\-]{1,9})\)")
STOP = {"the", "a", "an", "of", "for", "and", "in", "on", "to", "with", "by", "we", "our", "this", "is", "are", "as", "using", "use", "called", "named", "termed", "known"}


def clean(text):
    text = re.sub(r"(\w)\x02\s*\n?\s*(\w)", r"\1\2", text)  # PDFium soft-hyphen marker (S2 finding 2)
    return text.replace("\f", "\n")


def long_form(words, abbr):
    """Shortest tail of `words` whose initials can spell the abbreviation."""
    ws = [w for w in re.split(r"[\s\-]+", words) if w]
    letters = [c for c in abbr.lower() if c.isalpha()]
    for n in range(1, len(ws) + 1):
        tail = ws[-n:]
        initials = "".join(w[0].lower() for w in tail if w.lower() not in STOP)
        if initials == "".join(letters) or ("".join(letters).startswith(initials) and len(initials) >= len(letters) - 1 and n >= 2):
            if initials == "".join(letters):
                return " ".join(tail)
    return None


def passages(text, term, limit=3):
    esc = re.escape(term)
    find = re.compile(rf"(?<![\w]){esc}(?![\w])")
    defines = re.compile(rf"{esc}\s*(?:\([^)]{{1,40}}\)\s*)?(?:is|are|was|denotes?|refers? to|stands? for|means|represents?|=|:)|"
                         rf"(?:called|termed|known as|denoted(?: by)?|defined as)\s+(?:the\s+)?{esc}|\({esc}\)")
    found = []
    for m in find.finditer(text):
        snip = re.sub(r"\s+", " ", text[max(0, m.start() - 260): m.end() + 260]).strip()
        found.append((bool(defines.search(snip)), snip))
        if len(found) >= 40:
            break
    seen, out = set(), []
    for _, snip in sorted(found, key=lambda x: not x[0]):
        if snip[:120] not in seen:
            seen.add(snip[:120])
            out.append(snip)
    return out[:limit]


def main():
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    cases = []
    for f in sorted(src.glob("*.pdfium.txt")):
        paper = f.name[: -len(".pdfium.txt")]
        if paper.startswith("fawiki"):
            continue
        text = clean(f.read_text("utf-8"))
        per_paper = 0
        seen = set()
        for m in ABBR.finditer(text):
            abbr = m.group(2)
            if abbr in seen or abbr.isdigit() or len(abbr) < 2 or abbr.lower() in {"eq", "fig", "ie", "eg", "et", "al"}:
                continue
            lf = long_form(m.group(1), abbr)
            if not lf or len(lf) < 6:
                continue
            # a later use of the abbreviation, away from the definition
            later = [u for u in re.finditer(rf"(?<![\w]){re.escape(abbr)}(?![\w])", text) if u.start() > m.end() + 400]
            if not later:
                continue
            u = later[len(later) // 2]
            seen.add(abbr)
            cases.append({
                "id": f"{paper}:{abbr}", "paper": paper, "term": abbr, "expected": lf,
                "before": re.sub(r"\s+", " ", text[max(0, u.start() - 500): u.start()]).strip(),
                "after": re.sub(r"\s+", " ", text[u.end(): u.end() + 500]).strip(),
                "passages": passages(text, abbr),
            })
            per_paper += 1
            if per_paper >= 4:
                break
    dst.write_text(json.dumps(cases, indent=1, ensure_ascii=False), "utf-8")
    print(len(cases), "cases from", len({c["paper"] for c in cases}), "papers")
    for c in cases:
        print(f"  {c['term']:8} = {c['expected']}")


if __name__ == "__main__":
    main()
