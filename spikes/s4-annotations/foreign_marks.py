"""Marks made by another tool: MuPDF's own annotation writer (via PyMuPDF; measurement only).

Writes highlight, underline, strike-out, note + reply, free text, ink, square and an arrow line on
page 1 of a copy, saved incrementally, so our reader (PDFium) can be tested on marks it did not make.
usage: python -I -X utf8 foreign_marks.py <original.pdf> <out.pdf>
"""
import shutil
import sys

import pymupdf

src, dst = sys.argv[1], sys.argv[2]
shutil.copyfile(src, dst)
doc = pymupdf.open(dst)
page = doc[0]
words = [w for w in page.get_text("words") if len(w[4]) >= 6]
quad = lambda w: pymupdf.Rect(w[:4]).quad

h = page.add_highlight_annot(quad(words[40])); h.set_info(title="Other Tool", content="highlight from another tool", subject="definition"); h.update()
u = page.add_underline_annot(quad(words[70])); u.set_info(title="Other Tool"); u.update()
s = page.add_strikeout_annot(quad(words[100])); s.set_info(title="Other Tool"); s.update()
n = page.add_text_annot((page.rect.width - 40, 300), "note from another tool — یادداشت", icon="Comment"); n.set_info(title="Other Tool"); n.update()
reply = page.add_text_annot((page.rect.width - 40, 300), "reply from another tool"); reply.set_info(title="Other Tool"); reply.set_irt_xref(n.xref); reply.update()
f = page.add_freetext_annot(pymupdf.Rect(60, 700, 260, 722), "free text from another tool", fontsize=9); f.set_info(title="Other Tool"); f.update()
i = page.add_ink_annot([[(300, 720), (320, 700), (340, 720), (360, 700)]]); i.set_info(title="Other Tool"); i.update()
q = page.add_rect_annot(pymupdf.Rect(50, 60, 300, 120)); q.set_info(title="Other Tool"); q.update()
l = page.add_line_annot((80, 600), (180, 560)); l.set_line_ends(pymupdf.PDF_ANNOT_LE_NONE, pymupdf.PDF_ANNOT_LE_OPEN_ARROW); l.set_info(title="Other Tool"); l.update()
doc.saveIncr()
print("wrote", len(list(page.annots())), "annotations (incremental) to", dst)
