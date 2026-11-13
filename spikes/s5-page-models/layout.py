"""Spike S5 — layout models on CPU: speed, and recall of what the paper itself announces.

Pages are rendered with PDFium (pypdfium2 — the engine the product uses). For each page and model:
render → model → boxes in PDF points. Truth comes from the paper's own text (PDFium characters,
spike S2 output): every "Figure N"/"Fig. N"/"Table N" caption must have a detected picture/table
next to it, and every equation number "(N)" standing alone at a line end must have a detected
formula in the same horizontal band.

usage: python -I -X utf8 layout.py <model: heron|pp> <threads> <s2-out-dir> <out.json> <pdf>...
"""
import json
import re
import sys
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
import pypdfium2 as pdfium
from PIL import Image

STORE = Path(r"D:\local LLM\LLM-Models\page-models")
HERON = ["caption", "footnote", "formula", "list_item", "page_footer", "page_header", "picture", "section_header",
         "table", "text", "title", "document_index", "code", "checkbox_selected", "checkbox_unselected", "form", "key_value_region"]
PP = ["paragraph_title", "image", "text", "number", "abstract", "content", "figure_title", "formula", "table", "reference",
      "doc_title", "footnote", "header", "algorithm", "footer", "seal", "chart", "formula_number", "aside_text", "reference_content"]
FIGURE = {"picture", "image", "chart"}
TABLE = {"table"}
FORMULA = {"formula"}


class Model:
    def __init__(self, name, threads):
        o = ort.SessionOptions()
        o.intra_op_num_threads = threads
        o.log_severity_level = 3
        self.name = name
        path = STORE / {"heron": "heron/model.onnx", "heron-int8": "heron/model.int8.onnx",
                        "egret": "egret-medium/model.onnx", "pp": "pp-doclayout-plus-l/inference.onnx"}[name]
        t = time.perf_counter()
        self.s = ort.InferenceSession(str(path), o, providers=["CPUExecutionProvider"])
        self.load_s = time.perf_counter() - t

    def __call__(self, img: Image.Image, thr=0.4):
        w, h = img.size
        if self.name in ("heron", "heron-int8"):
            x = np.asarray(img.resize((640, 640), Image.BILINEAR), dtype=np.uint8).transpose(2, 0, 1)[None]
            labels, boxes, scores = self.s.run(None, {"images": x, "orig_target_sizes": np.array([[w, h]], dtype=np.int64)})
            out = [(HERON[int(l)], float(s), *map(float, b)) for l, b, s in zip(labels[0], boxes[0], scores[0]) if s >= thr]
        elif self.name == "egret":
            # Hugging Face RT-DETR format: rescale to 0..1, no normalisation; sigmoid scores, boxes cx,cy,w,h in 0..1
            x = (np.asarray(img.resize((640, 640), Image.BILINEAR), dtype=np.float32) / 255.0).transpose(2, 0, 1)[None]
            logits, boxes = self.s.run(None, {"pixel_values": x})
            prob = 1.0 / (1.0 + np.exp(-logits[0]))
            cls, sc = prob.argmax(-1), prob.max(-1)
            out = []
            for c, s, (cx, cy, bw, bh) in zip(cls, sc, boxes[0]):
                if s >= thr:
                    out.append((HERON[int(c)], float(s), (cx - bw / 2) * w, (cy - bh / 2) * h, (cx + bw / 2) * w, (cy + bh / 2) * h))
        else:
            x = (np.asarray(img.resize((800, 800), Image.BICUBIC), dtype=np.float32) / 255.0).transpose(2, 0, 1)[None]
            res = self.s.run(None, {"image": x, "im_shape": np.array([[800, 800]], np.float32),
                                    "scale_factor": np.array([[800 / h, 800 / w]], np.float32)})[0]
            out = [(PP[int(r[0])], float(r[1]), *map(float, r[2:6])) for r in res if r[1] >= thr and 0 <= int(r[0]) < len(PP)]
        return out  # (label, score, x0, y0, x1, y1) in image pixels, origin top-left


def lines_of(chars, page_h):
    """Visual line segments from PDFium characters: (text, x0, top, x1, bottom) in points, top-left origin."""
    rows = {}
    for c in chars:
        if not c[0].strip():
            continue
        rows.setdefault(round(c[2] / 2.0), []).append(c)
    segs = []
    for row in rows.values():
        row.sort(key=lambda c: c[3])
        cur = [row[0]]
        for c in row[1:]:
            if c[3] - cur[-1][5] > 12:            # a column gap splits the row
                segs.append(cur)
                cur = []
            cur.append(c)
        segs.append(cur)
    out = []
    for s in segs:
        text = "".join(c[0] for c in s)
        x0, x1 = min(c[3] for c in s), max(c[5] for c in s)
        top, bottom = page_h - max(c[6] for c in s), page_h - min(c[4] for c in s)
        out.append((text, x0, top, x1, bottom))
    return out


CAPTION = re.compile(r"^(Fig\.?|Figure|FIGURE|Table|TABLE)\s*(\d+|[IVX]+)\b")
EQNUM = re.compile(r"^\((\d{1,3}(?:\.\d{1,3}){0,2}[a-z]?)\)$")   # (7), (7a), (2.1), (A.3) is not matched


def truth(lines, page_w):
    caps, eqs = [], []
    for t, x0, top, x1, bottom in lines:
        m = CAPTION.match(t.replace(" ", " "))
        if m:
            caps.append(("table" if m.group(1).lower().startswith("tab") else "figure", (x0, top, x1, bottom), t[:40]))
        elif EQNUM.match(t.strip()) and x0 > page_w * 0.3:
            eqs.append(((x0, top, x1, bottom), t.strip()))
    return caps, eqs


def overlap_1d(a0, a1, b0, b1):
    return max(0.0, min(a1, b1) - max(a0, b0))


def score(caps, eqs, boxes):
    """boxes: (label, score, x0, top, x1, bottom) in points."""
    cap_hit, eq_hit = [], []
    for kind, (x0, top, x1, bottom), t in caps:
        want = TABLE if kind == "table" else FIGURE
        near = [b for b in boxes if b[0] in want and overlap_1d(x0, x1, b[2], b[4]) > 0.3 * (x1 - x0)
                and (abs(b[5] - top) < 40 or abs(b[3] - bottom) < 40 or overlap_1d(top, bottom, b[3], b[5]) > 0)]
        cap_hit.append({"kind": kind, "caption": t, "found": bool(near)})
    for (x0, top, x1, bottom), t in eqs:
        band = [b for b in boxes if b[0] in FORMULA and overlap_1d(top, bottom, b[3], b[5]) > 0.5 * (bottom - top)]
        eq_hit.append({"number": t, "found": bool(band)})
    return cap_hit, eq_hit


def main():
    name, threads, s2, out_path, pdfs = sys.argv[1], int(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]), sys.argv[5:]
    model = Model(name, threads)
    scale = 2.0  # 144 dpi render; models resize internally
    results = {"model": name, "threads": threads, "load_s": round(model.load_s, 2), "papers": []}
    for pdf in pdfs:
        stem = Path(pdf).stem
        chars = json.loads((s2 / f"{stem}.pdfium.chars.json").read_text("utf-8"))
        doc = pdfium.PdfDocument(pdf)
        n = min(len(doc), 20)
        per_page, caps_all, eqs_all, counts = [], [], [], {}
        for i in range(n):
            page = doc[i]
            pw, ph = page.get_size()
            t0 = time.perf_counter()
            img = page.render(scale=scale).to_pil().convert("RGB")
            t1 = time.perf_counter()
            raw = model(img)
            t2 = time.perf_counter()
            boxes = [(l, s, x0 / scale, y0 / scale, x1 / scale, y1 / scale) for l, s, x0, y0, x1, y1 in raw]
            for b in boxes:
                counts[b[0]] = counts.get(b[0], 0) + 1
            caps, eqs = truth(lines_of(chars[i]["chars"], ph), pw)
            ch, eh = score(caps, eqs, boxes)
            caps_all += ch
            eqs_all += eh
            per_page.append({"page": i + 1, "render_ms": round(1000 * (t1 - t0)), "model_ms": round(1000 * (t2 - t1)), "boxes": len(boxes)})
        cap_found = sum(c["found"] for c in caps_all)
        eq_found = sum(e["found"] for e in eqs_all)
        results["papers"].append({
            "paper": stem, "pages": n,
            "model_ms_median": sorted(p["model_ms"] for p in per_page)[n // 2],
            "render_ms_median": sorted(p["render_ms"] for p in per_page)[n // 2],
            "total_s": round(sum(p["model_ms"] + p["render_ms"] for p in per_page) / 1000, 2),
            "captions": len(caps_all), "captions_found": cap_found,
            "equation_numbers": len(eqs_all), "equations_found": eq_found,
            "missed_captions": [c["caption"] for c in caps_all if not c["found"]][:6],
            "label_counts": counts,
        })
        r = results["papers"][-1]
        print(f"{name:5} t={threads} {stem:34} pages={n:2} model={r['model_ms_median']:4} ms/page render={r['render_ms_median']:3} ms "
              f"total={r['total_s']:5}s captions {cap_found}/{len(caps_all)} equations {eq_found}/{len(eqs_all)}", flush=True)
    out_path.write_text(json.dumps(results, indent=1, ensure_ascii=False), "utf-8")


if __name__ == "__main__":
    main()
