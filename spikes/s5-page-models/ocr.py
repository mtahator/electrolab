"""Spike S5 — OCR of image-only pages with PP-OCRv5 mobile (ONNX, CPU).

Pages are rendered to images with PDFium (so the PDF's own text layer is not used), then:
  detection (DB): probability map → threshold → contours → expanded rotated boxes
  recognition (CTC): crop each line, height 48, greedy decode with the model's character list.
Scored against reference text: NACA's existing OCR layer (English) or the correct Persian text
from spike S2's PyMuPDF output (the Wikipedia article). Word-multiset F1, timings per page.

usage: python -I -X utf8 ocr.py <en|arabic> <threads> <reference.txt> <out.json> <pdf> <first-page> <n-pages>
"""
import collections
import json
import re
import sys
import time
from pathlib import Path

import cv2
import numpy as np
import onnxruntime as ort
import pypdfium2 as pdfium
import yaml

STORE = Path(r"D:\local LLM\LLM-Models\page-models")


def session(path, threads):
    o = ort.SessionOptions()
    o.intra_op_num_threads = threads
    o.log_severity_level = 3
    return ort.InferenceSession(str(path), o, providers=["CPUExecutionProvider"])


def charset(yml):
    cfg = yaml.safe_load(Path(yml).read_text("utf-8"))
    chars = cfg["PostProcess"]["character_dict"]
    return ["<blank>"] + list(chars) + [" "]


def detect(det, img, max_side=1280, thr=0.3, box_thr=0.6, unclip=1.6):
    h, w = img.shape[:2]
    s = min(1.0, max_side / max(h, w))
    nh, nw = max(32, int(round(h * s / 32)) * 32), max(32, int(round(w * s / 32)) * 32)
    x = cv2.resize(img, (nw, nh)).astype(np.float32) / 255.0
    x = (x - np.array([0.485, 0.456, 0.406], np.float32)) / np.array([0.229, 0.224, 0.225], np.float32)
    prob = det.run(None, {"x": x.transpose(2, 0, 1)[None]})[0][0, 0]
    mask = (prob > thr).astype(np.uint8)
    contours, _ = cv2.findContours(mask, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
    boxes = []
    for c in contours:
        if cv2.contourArea(c) < 6:
            continue
        x0, y0, bw, bh = cv2.boundingRect(c)
        if float(prob[y0:y0 + bh, x0:x0 + bw][mask[y0:y0 + bh, x0:x0 + bw] > 0].mean()) < box_thr:
            continue
        (cx, cy), (rw, rh), ang = cv2.minAreaRect(c)
        d = (rw * rh) * unclip / max(1e-6, 2 * (rw + rh))      # DB "unclip": grow by area/perimeter
        rect = ((cx / nw * w, cy / nh * h), ((rw + 2 * d) / nw * w, (rh + 2 * d) / nh * h), ang)
        boxes.append(cv2.boxPoints(rect))
    # reading order: top to bottom, then left to right
    boxes.sort(key=lambda b: (round(b[:, 1].mean() / 12), b[:, 0].mean()))
    return boxes


def crop(img, pts):
    pts = pts.astype(np.float32)
    s = pts.sum(1); d = np.diff(pts, axis=1).ravel()
    tl, br, tr, bl = pts[s.argmin()], pts[s.argmax()], pts[d.argmin()], pts[d.argmax()]
    w = int(max(np.linalg.norm(tr - tl), np.linalg.norm(br - bl)))
    h = int(max(np.linalg.norm(bl - tl), np.linalg.norm(br - tr)))
    if w < 2 or h < 2:
        return None
    m = cv2.getPerspectiveTransform(np.array([tl, tr, br, bl]), np.array([[0, 0], [w, 0], [w, h], [0, h]], np.float32))
    out = cv2.warpPerspective(img, m, (w, h), borderMode=cv2.BORDER_REPLICATE)
    return np.rot90(out) if h > 1.5 * w else out


def recognise(rec, chars, crops, batch=8):
    # Batch lines of similar width (as PaddleOCR does): padding every line to the widest one
    # in an unsorted batch made recognition 3–5× slower in the first run.
    order = sorted(range(len(crops)), key=lambda k: crops[k].shape[1] / crops[k].shape[0])
    texts = [""] * len(crops)
    for i in range(0, len(order), batch):
        ids = order[i:i + batch]
        part = [crops[k] for k in ids]
        widths = [max(16, int(48 * c.shape[1] / c.shape[0])) for c in part]
        W = min(3200, max(widths))
        x = np.zeros((len(part), 3, 48, W), np.float32)
        for k, (c, cw) in enumerate(zip(part, widths)):
            r = cv2.resize(c, (min(cw, W), 48)).astype(np.float32) / 255.0
            x[k, :, :, : r.shape[1]] = ((r - 0.5) / 0.5).transpose(2, 0, 1)
        probs = rec.run(None, {"x": x})[0]
        for k, p in zip(ids, probs):
            idx = p.argmax(-1)
            out, prev = [], 0
            for j in idx:
                if j != prev and j != 0 and j < len(chars):
                    out.append(chars[j])
                prev = j
            texts[k] = "".join(out)
    return texts


def words(s):
    return re.findall(r"\w+", s.lower().replace("\u200c", ""))


def f1(a, b):
    ca, cb = collections.Counter(words(a)), collections.Counter(words(b))
    common = sum((ca & cb).values())
    if not common:
        return 0.0
    p, r = common / sum(ca.values()), common / sum(cb.values())
    return round(2 * p * r / (p + r), 3)


def main():
    lang, threads, ref_path, out_path, pdf, first, n = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5], int(sys.argv[6]), int(sys.argv[7])
    det = session(STORE / "ppocrv5-det/inference.onnx", threads)
    rec_dir = STORE / f"ppocrv5-rec-{lang}"
    rec, chars = session(rec_dir / "inference.onnx", threads), charset(rec_dir / "inference.yml")
    ref_pages = Path(ref_path).read_text("utf-8").split("\f")
    doc = pdfium.PdfDocument(pdf)
    rows = []
    for i in range(first - 1, first - 1 + n):
        t0 = time.perf_counter()
        img = np.asarray(doc[i].render(scale=200 / 72).to_pil().convert("RGB"))
        t1 = time.perf_counter()
        boxes = detect(det, img)
        t2 = time.perf_counter()
        crops = [c for c in (crop(img, b) for b in boxes) if c is not None]
        texts = recognise(rec, chars, crops)
        t3 = time.perf_counter()
        text = "\n".join(texts)
        ref = ref_pages[i] if i < len(ref_pages) else ""
        row = {"page": i + 1, "lines": len(texts), "render_ms": round(1000 * (t1 - t0)), "detect_ms": round(1000 * (t2 - t1)),
               "recognise_ms": round(1000 * (t3 - t2)), "f1": f1(text, ref)}
        if lang == "arabic":   # the recogniser may emit right-to-left lines in visual order: score both
            row["f1_lines_reversed"] = f1("\n".join(t[::-1] for t in texts), ref)
        row["sample"] = text[:200]
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if k != "sample"}), flush=True)
    Path(out_path).write_text(json.dumps({"lang": lang, "threads": threads, "pdf": Path(pdf).name, "pages": rows}, indent=1, ensure_ascii=False), "utf-8")


if __name__ == "__main__":
    main()
