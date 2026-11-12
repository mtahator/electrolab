"""Reference text from PyMuPDF, called the way ElectroChat 2.5 calls it
(`page.get_text("text", sort=False)`, engine/pdf_worker.py). Measurement only:
PyMuPDF is AGPL and never ships in the product.

usage: python -I pymupdf_ref.py <out-dir> <pdf>...
"""
import json
import sys
import time
from pathlib import Path

import pymupdf


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for pdf in map(Path, sys.argv[2:]):
        try:
            t = time.perf_counter()
            doc = pymupdf.open(pdf)
            open_ms = (time.perf_counter() - t) * 1000
            t = time.perf_counter()
            text = "".join(page.get_text("text", sort=False) + "\f" for page in doc)
            text_ms = (time.perf_counter() - t) * 1000
            (out / f"{pdf.stem}.pymupdf.txt").write_text(text, "utf-8")
            print(json.dumps({"file": pdf.stem, "pages": doc.page_count, "open_ms": round(open_ms, 2),
                              "text_ms": round(text_ms, 2),
                              "text_ms_per_100p": round(text_ms / max(doc.page_count, 1) * 100, 2),
                              "pymupdf": pymupdf.VersionBind}))
        except Exception as e:  # report and continue: one bad file must not stop the run
            print(json.dumps({"file": pdf.stem, "error": repr(e)}))


if __name__ == "__main__":
    main()
