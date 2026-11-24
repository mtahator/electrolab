"""Non-academic test documents (fixtures/documents): written by this script, so they carry no
third-party licence. Run with `python fixtures/documents/generate.py` (needs reportlab); the PDFs
are committed, so tests do not need Python.

Each covers layouts a research paper does not have:
- product-manual.pdf   cover, contents page, numbered steps, warning boxes, spec and
                       troubleshooting tables, running header and footer, cross references
- poster-research.pdf  A0 poster, three columns of boxed sections, charts, small reference list
- poster-event.pdf     A3 event flyer: big display type, few words, no sections
- slides.pdf           16:9 slide deck: one title per slide, bullets, a chart, slide numbers
- brochure.pdf         landscape tri-fold, three panels per page
- report-business.pdf  quarterly report: headings, KPI table, bullet lists, no references
- rotated-pages.pdf    pages turned 0, 90, 180 and 270 degrees (/Rotate), for page geometry
- mixed-pages.pdf      sizes and directions mixed: inherited boxes, /Rotate, landscape, A3, crop
- book-800.pdf         an 800-page book (review 2026-11-15): chapters, two-level numbering
                       ("Figure 3.2", "Table 12.4"), sentences that start with a label, tables
                       continued on the next page, charts, references, dense text — the large
                       document every budget is measured on (design language §17.3)
"""

from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.pagesizes import A0, A3, landscape, letter
from reportlab.lib.units import mm
from reportlab.pdfgen import canvas

OUT = Path(__file__).parent
INFO = {"author": "ElectroPaper fixtures", "creator": "fixtures/documents/generate.py"}


def new(name, size, title):
    c = canvas.Canvas(str(OUT / name), pagesize=size, invariant=1)
    c.setTitle(title)
    c.setAuthor(INFO["author"])
    c.setCreator(INFO["creator"])
    return c


def para(c, x, y, width, text, font="Helvetica", size=10, leading=None):
    """Simple word-wrapped paragraph; returns the y below it."""
    leading = leading or size * 1.35
    c.setFont(font, size)
    line = ""
    for word in text.split():
        trial = f"{line} {word}".strip()
        if c.stringWidth(trial, font, size) > width and line:
            c.drawString(x, y, line)
            y -= leading
            line = word
        else:
            line = trial
    if line:
        c.drawString(x, y, line)
        y -= leading
    return y


def table(c, x, y, widths, rows, size=9, header=True):
    h = size * 1.9
    for r, row in enumerate(rows):
        cx = x
        if header and r == 0:
            c.setFillColor(colors.HexColor("#e8ecf2"))
            c.rect(x, y - h + 3, sum(widths), h, stroke=0, fill=1)
            c.setFillColor(colors.black)
        for w, cell in zip(widths, row):
            c.setFont("Helvetica-Bold" if header and r == 0 else "Helvetica", size)
            c.drawString(cx + 4, y - size, str(cell))
            cx += w
        c.setStrokeColor(colors.HexColor("#b0b6c0"))
        c.line(x, y - h + 3, x + sum(widths), y - h + 3)
        y -= h
    return y - 6


def bar_chart(c, x, y, w, h, values, labels, color="#3b6fb6"):
    c.setStrokeColor(colors.black)
    c.line(x, y, x + w, y)
    c.line(x, y, x, y + h)
    top = max(values)
    bw = w / (len(values) * 1.6)
    for i, (v, lab) in enumerate(zip(values, labels)):
        bx = x + bw * 0.6 + i * bw * 1.6
        c.setFillColor(colors.HexColor(color))
        c.rect(bx, y, bw, h * v / top * 0.9, stroke=0, fill=1)
        c.setFillColor(colors.black)
        c.setFont("Helvetica", max(6, h / 16))
        c.drawCentredString(bx + bw / 2, y - h / 12, lab)


# ---------------------------------------------------------------- product manual -------------
def manual():
    W, H = letter
    c = new("product-manual.pdf", letter, "Brewline B200 Coffee Grinder — User Manual")
    # Cover
    c.setFillColor(colors.HexColor("#1f2a37"))
    c.rect(0, H - 260, W, 260, stroke=0, fill=1)
    c.setFillColor(colors.white)
    c.setFont("Helvetica-Bold", 34)
    c.drawString(60, H - 140, "Brewline B200")
    c.setFont("Helvetica", 18)
    c.drawString(60, H - 172, "Burr Coffee Grinder")
    c.setFillColor(colors.black)
    c.setFont("Helvetica-Bold", 22)
    c.drawString(60, H - 330, "User Manual")
    c.setFont("Helvetica", 11)
    c.drawString(60, H - 352, "Model B200-EU · Version 2.1 · Read all instructions before use.")
    c.showPage()

    pages = []

    def page(title_parts):
        pages.append(title_parts)

    def frame(n):
        c.setFont("Helvetica", 8)
        c.setFillColor(colors.HexColor("#555555"))
        c.drawString(60, H - 36, "Brewline B200 — User Manual")
        c.drawRightString(W - 60, 30, f"Page {n}")
        c.setFillColor(colors.black)

    # Contents
    frame(2)
    c.setFont("Helvetica-Bold", 18)
    c.drawString(60, H - 90, "Contents")
    toc = [("1", "Safety instructions", 3), ("2", "Parts and controls", 3), ("3", "Getting started", 4),
           ("4", "Cleaning and care", 5), ("5", "Troubleshooting", 5), ("6", "Technical data", 6)]
    y = H - 125
    for num, name, pg in toc:
        c.setFont("Helvetica", 11)
        c.drawString(70, y, f"{num}  {name}")
        c.drawRightString(W - 70, y, str(pg))
        y -= 20
    c.showPage()

    def heading(y, text, size=15):
        c.setFont("Helvetica-Bold", size)
        c.drawString(60, y, text)
        return y - size * 1.8

    def warning(y, text):
        c.setFillColor(colors.HexColor("#fff4d6"))
        c.setStrokeColor(colors.HexColor("#d9a400"))
        c.rect(60, y - 46, W - 120, 52, stroke=1, fill=1)
        c.setFillColor(colors.black)
        c.setFont("Helvetica-Bold", 10)
        c.drawString(72, y - 12, "WARNING")
        para(c, 72, y - 26, W - 150, text, size=9)
        return y - 62

    # Page 3
    frame(3)
    y = heading(H - 80, "1 Safety instructions")
    y = para(c, 60, y, W - 120, "This appliance is intended for grinding roasted coffee beans in households. "
             "Do not use it for spices, grain or any other material. Keep the grinder away from water and "
             "children. Unplug the grinder before you open the bean hopper or the burr chamber.")
    y = warning(y - 6, "The burrs are sharp. Never put your fingers into the burr chamber while the "
                "grinder is plugged in. Risk of injury.")
    y = heading(y - 4, "2 Parts and controls")
    y = para(c, 60, y, W - 120, "The parts are shown in Figure 1. The grind setting dial (C) has 40 steps; "
             "lower numbers give finer coffee. The timer (D) sets the grinding time from 1 to 60 seconds.")
    # Figure 1: diagram
    c.setStrokeColor(colors.black)
    c.rect(220, y - 170, 120, 150)
    c.circle(280, y - 40, 22)
    c.rect(250, y - 120, 60, 40)
    for lab, lx, ly in [("A Bean hopper", 360, y - 40), ("B Burr chamber", 360, y - 100), ("C Grind dial", 360, y - 130), ("D Timer", 360, y - 160)]:
        c.line(340, ly + 3, lx - 4, ly + 3)
        c.setFont("Helvetica", 9)
        c.drawString(lx, ly, lab)
    c.setFont("Helvetica-Oblique", 9)
    c.drawString(60, y - 190, "Figure 1: Parts of the B200 grinder.")
    c.showPage()

    # Page 4: steps
    frame(4)
    y = heading(H - 80, "3 Getting started")
    y = heading(y, "3.1 Before first use", 12)
    y = para(c, 60, y, W - 120, "Remove all packing material and wipe the hopper with a dry cloth. "
             "See Section 4 for cleaning the burrs.")
    y = heading(y - 4, "3.2 Grinding coffee", 12)
    steps = [
        "Place the grinder on a dry, flat surface and plug it in.",
        "Fill the bean hopper (A) with up to 250 g of beans and close the lid.",
        "Turn the grind dial (C) to the setting for your brew method (see Table 1).",
        "Set the timer (D) and press the start button. The grinder stops by itself.",
        "Remove the grounds container and tap it gently to release the coffee.",
    ]
    for i, s in enumerate(steps, 1):
        c.setFont("Helvetica-Bold", 10)
        c.drawString(66, y, f"{i}.")
        y = para(c, 84, y, W - 150, s)
        y -= 2
    c.setFont("Helvetica-Bold", 10)
    c.drawString(60, y - 6, "Table 1: Recommended grind settings")
    y = table(c, 60, y - 14, [200, 120, 150], [["Brew method", "Setting", "Grind time (s)"],
                                                 ["Espresso", "4–8", "12"], ["Filter", "18–24", "20"],
                                                 ["French press", "30–36", "25"], ["Cold brew", "36–40", "30"]])
    c.showPage()

    # Page 5
    frame(5)
    y = heading(H - 80, "4 Cleaning and care")
    y = para(c, 60, y, W - 120, "Clean the burrs every two weeks. Unplug the grinder, empty the hopper, turn "
             "the upper burr anticlockwise and lift it out. Brush away coffee residue. Never wash the burrs with water.")
    y = warning(y - 6, "Do not put any part of the grinder in a dishwasher.")
    y = heading(y - 4, "5 Troubleshooting")
    y = table(c, 60, y, [170, 300], [["Problem", "What to do"],
                                     ["The grinder does not start", "Check that the hopper lid is closed."],
                                     ["The coffee is too coarse", "Turn the grind dial to a lower number."],
                                     ["Grinding is loud", "Clean the burrs (Section 4)."],
                                     ["Error light blinks", "Let the motor cool for 15 minutes."]])
    c.showPage()

    # Page 6
    frame(6)
    y = heading(H - 80, "6 Technical data")
    y = table(c, 60, y, [200, 200], [["Property", "Value"], ["Rated voltage", "220–240 V"],
                                     ["Power", "150 W"], ["Hopper capacity", "250 g"],
                                     ["Weight", "1.9 kg"], ["Dimensions", "13 × 19 × 33 cm"]])
    y = para(c, 60, y - 10, W - 120, "Specifications may change without notice. Dispose of the appliance at a "
             "collection point for electrical equipment.", size=9)
    c.showPage()
    c.save()


# ---------------------------------------------------------------- research poster -------------
def poster_research():
    W, H = A0
    c = new("poster-research.pdf", A0, "Cooling Shallow Lakes with Floating Wetlands — Poster")
    c.setFillColor(colors.HexColor("#0f4c5c"))
    c.rect(0, H - 360, W, 360, stroke=0, fill=1)
    c.setFillColor(colors.white)
    c.setFont("Helvetica-Bold", 96)
    c.drawCentredString(W / 2, H - 190, "Cooling Shallow Lakes with Floating Wetlands")
    c.setFont("Helvetica", 44)
    c.drawCentredString(W / 2, H - 280, "R. Alavi · M. Chen · S. Okafor — Lakeside Institute")
    c.setFillColor(colors.black)
    col_w = (W - 4 * 60) / 3
    sections = [
        [("Introduction", "Shallow lakes warm quickly in summer, which favours algal blooms. Floating "
          "wetlands shade the water and take up nutrients. We asked how much they cool the surface layer "
          "and whether the effect lasts through heat waves [1, 2]."),
         ("Methods", "Six mesocosms of 4 m² each were covered 0, 20 or 40 % by floating wetlands. "
          "Temperature loggers at 10 cm and 50 cm depth recorded every 10 minutes from June to August. "
          "Shading was measured with paired light sensors.")],
        [("Results", "Surface water under 40 % cover was 2.1 °C cooler on average (Figure 1), and the "
          "difference grew to 3.4 °C during the July heat wave. Chlorophyll fell by a third (Table 1).")],
        [("Conclusions", "Floating wetlands are a low-cost way to cool small lakes. Cover above 30 % gives "
          "most of the benefit; more cover mainly reduces oxygen at depth."),
         ("References", "[1] Smith J, Lee K. Lake heat waves. Limnology. 2021;12:44–51.  "
          "[2] Ortega P. Floating islands. Ecol Eng. 2019;8:101–9.")],
    ]
    for i, col in enumerate(sections):
        x = 60 + i * (col_w + 60)
        y = H - 440
        for title, text in col:
            c.setFillColor(colors.HexColor("#e6f0f2"))
            c.rect(x, y - 54, col_w, 70, stroke=0, fill=1)
            c.setFillColor(colors.HexColor("#0f4c5c"))
            c.setFont("Helvetica-Bold", 48)
            c.drawString(x + 24, y - 36, title)
            c.setFillColor(colors.black)
            if title == "References":
                y -= 110
                for ref in text.split("  "):
                    y = para(c, x + 24, y, col_w - 48, ref, size=26, leading=36) - 10
                continue
            y = para(c, x + 24, y - 110, col_w - 48, text, size=30, leading=42) - 30
            if title == "Results":
                bar_chart(c, x + 80, y - 600, col_w - 160, 520, [24.8, 23.9, 22.7], ["0 %", "20 %", "40 %"])
                c.setFont("Helvetica-Oblique", 28)
                c.drawString(x + 24, y - 680, "Figure 1: Mean surface temperature (°C) by cover.")
                y = table(c, x + 24, y - 740, [col_w * 0.45, col_w * 0.45], [["Cover", "Chlorophyll (µg/L)"],
                          ["0 %", "18.2"], ["20 %", "14.9"], ["40 %", "12.1"]], size=28)
                c.setFont("Helvetica-Oblique", 28)
                c.drawString(x + 24, y - 20, "Table 1: Chlorophyll a in August.")
                y -= 80
    c.showPage()
    c.save()


# ---------------------------------------------------------------- event poster ----------------
def poster_event():
    W, H = A3
    c = new("poster-event.pdf", A3, "Night Market — Riverside Square")
    c.setFillColor(colors.HexColor("#2b1d4a"))
    c.rect(0, 0, W, H, stroke=0, fill=1)
    c.setFillColor(colors.HexColor("#ffcc4d"))
    c.setFont("Helvetica-Bold", 88)
    c.drawCentredString(W / 2, H - 220, "NIGHT")
    c.drawCentredString(W / 2, H - 320, "MARKET")
    c.setFillColor(colors.white)
    c.setFont("Helvetica", 30)
    c.drawCentredString(W / 2, H - 420, "Riverside Square · Every Friday in June")
    c.setFont("Helvetica", 22)
    for i, line in enumerate(["Street food from 30 kitchens", "Live music from 7 pm", "Free entry · Family friendly"]):
        c.drawCentredString(W / 2, H - 560 - i * 44, line)
    c.setFont("Helvetica-Bold", 26)
    c.drawCentredString(W / 2, 90, "riverside-market.example")
    c.showPage()
    c.save()


# ---------------------------------------------------------------- slides ----------------------
def slides():
    W, H = 960, 540
    c = new("slides.pdf", (W, H), "Quarterly Product Review — Slides")
    deck = [
        ("Quarterly Product Review", ["Q2 · Product team", "Prepared for the leadership meeting"]),
        ("Agenda", ["Highlights of the quarter", "Customer feedback", "Roadmap for Q3", "Questions"]),
        ("Highlights", ["Active users grew 18 % to 42,000", "Two major releases shipped on time", "Support tickets fell by 25 %"]),
        ("Monthly active users", None),
        ("Customer feedback", ["Faster search is the most requested feature", "Offline mode praised by field teams", "Onboarding still too long"]),
        ("Roadmap for Q3", ["Search rewrite (July)", "Shorter onboarding (August)", "Team workspaces (September)"]),
        ("Questions?", ["Thank you"]),
    ]
    for n, (title, bullets) in enumerate(deck, 1):
        c.setFillColor(colors.HexColor("#f6f7f9"))
        c.rect(0, 0, W, H, stroke=0, fill=1)
        c.setFillColor(colors.HexColor("#3b6fb6"))
        c.rect(0, H - 8, W, 8, stroke=0, fill=1)
        c.setFillColor(colors.black)
        if n == 1:
            c.setFont("Helvetica-Bold", 48)
            c.drawCentredString(W / 2, H / 2 + 20, title)
            c.setFont("Helvetica", 22)
            for i, b in enumerate(bullets):
                c.drawCentredString(W / 2, H / 2 - 30 - i * 30, b)
        else:
            c.setFont("Helvetica-Bold", 34)
            c.drawString(60, H - 90, title)
            if bullets:
                for i, b in enumerate(bullets):
                    c.setFont("Helvetica", 24)
                    c.drawString(80, H - 170 - i * 50, "•  " + b)
            else:
                bar_chart(c, 120, 90, 720, 330, [31, 34, 36, 38, 40, 42], ["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
                c.setFont("Helvetica", 14)
                c.drawString(120, 50, "Thousands of users per month")
        c.setFont("Helvetica", 12)
        c.drawRightString(W - 30, 20, str(n))
        c.showPage()
    c.save()


# ---------------------------------------------------------------- brochure --------------------
def brochure():
    W, H = landscape(letter)
    c = new("brochure.pdf", landscape(letter), "Greenway Community Garden — Brochure")
    panel = W / 3
    pages = [
        [("Join the garden", "Greenway Community Garden has 60 plots for neighbours who want to grow their own "
          "vegetables, herbs and flowers. Plots are rented for one season at a time."),
         ("What you get", "A 3 × 4 m raised bed, water from April to October, shared tools and compost, and "
          "advice from experienced gardeners every Saturday morning."),
         ("Greenway Community Garden", "12 Linden Street · Open daily 7 am – 8 pm")],
        [("Plot fees", "Standard plot: 45 per season. Accessible raised plot: 30 per season. Reduced fees on request."),
         ("Garden rules", "Keep your plot tended. No chemical pesticides. Share the tools and return them clean. "
          "Respect quiet hours after 7 pm."),
         ("How to apply", "Fill in the form at the garden shed or write to the garden committee. "
          "New plots are given out in March.")],
    ]
    for content in pages:
        for i, (title, text) in enumerate(content):
            x = i * panel + 36
            c.setFillColor(colors.HexColor("#2f6b3a"))
            y = para(c, x, H - 80, panel - 72, title, font="Helvetica-Bold", size=18, leading=22)
            c.setFillColor(colors.black)
            para(c, x, y - 12, panel - 72, text, size=11, leading=16)
            if i < 2:
                c.setStrokeColor(colors.HexColor("#cccccc"))
                c.setDash(3, 3)
                c.line((i + 1) * panel, 30, (i + 1) * panel, H - 30)
                c.setDash()
        c.showPage()
    c.save()


# ---------------------------------------------------------------- business report -------------
def report():
    W, H = letter
    c = new("report-business.pdf", letter, "Northwind Bakery — Quarterly Report Q2")
    c.setFont("Helvetica-Bold", 24)
    c.drawString(60, H - 90, "Northwind Bakery")
    c.setFont("Helvetica", 14)
    c.drawString(60, H - 114, "Quarterly report · Second quarter")
    y = H - 160
    c.setFont("Helvetica-Bold", 15)
    c.drawString(60, y, "Summary")
    y = para(c, 60, y - 24, W - 120, "Sales rose 9 % over the first quarter, driven by the new rye range and the "
             "second shop on Harbour Road. Costs rose 4 %, mainly flour and energy. The bakery ended the quarter "
             "with a margin of 14 %.")
    c.setFont("Helvetica-Bold", 15)
    c.drawString(60, y - 10, "Key figures")
    y = table(c, 60, y - 24, [180, 100, 100, 100], [["Measure", "Q1", "Q2", "Change"],
              ["Revenue (thousand)", "412", "449", "+9 %"], ["Costs (thousand)", "362", "376", "+4 %"],
              ["Margin", "12 %", "14 %", "+2 pts"], ["Customers per day", "780", "860", "+10 %"]])
    c.setFont("Helvetica-Bold", 15)
    c.drawString(60, y - 10, "Next quarter")
    y -= 34
    for b in ["Open a stall at the Saturday farmers' market.", "Renegotiate the flour contract before August.",
              "Hire two apprentices for the early shift."]:
        y = para(c, 70, y, W - 140, "•  " + b) - 4
    c.showPage()
    c.save()


# ---------------------------------------------------------------- rotated pages --------------
def rotated():
    W, H = letter
    c = new("rotated-pages.pdf", letter, "Rotated pages")
    for angle in (0, 90, 180, 270):
        c.setPageRotation(angle)
        c.setFont("Helvetica-Bold", 20)
        c.drawString(72, H - 100, f"Page turned {angle} degrees")
        para(c, 72, H - 140, W - 144, "This text is drawn upright on the page; the page itself is turned "
             "by the viewer, so its words must be found in the turned position.")
        c.showPage()
    c.save()


# ---------------------------------------------------------------- mixed pages ----------------
def mixed():
    """Boxes inherited from the page tree, as LaTeX writes them; needs PyMuPDF."""
    import re

    import fitz
    d = fitz.open()
    A4 = (595.28, 841.89)
    #       name                     size          turn  own box  crop
    plan = [("Portrait", A4, 0, False, None), ("Turned 90, box inherited", A4, 90, False, None),
            ("Turned 270, box inherited", A4, 270, False, None), ("Landscape, own box", (841.89, 595.28), 0, True, None),
            ("A3 plan", (841.89, 1190.55), 0, True, None), ("Cropped", A4, 0, True, (100, 200, 500, 700)),
            ("Turned 180, box inherited", A4, 180, False, None)]
    for name, (w, h), turn, own, crop in plan:
        p = d.new_page(width=w, height=h)
        p.insert_text((120, 260), name, fontsize=24)
        p.insert_textbox((120, 300, w - 110, h - 150), "Every page keeps its own size and direction. " * 12, fontsize=11)
        if turn:
            p.set_rotation(turn)
        if crop:
            p.set_cropbox(fitz.Rect(crop))
        if not own:
            # Taken out, not set to null: a null box is not inherited by every reader.
            d.update_object(p.xref, re.sub(r"/MediaBox\s*\[[^\]]*\]", "", d.xref_object(p.xref)))
    parent = int(d.xref_get_key(d[0].xref, "Parent")[1].split()[0])
    d.xref_set_key(parent, "MediaBox", f"[0 0 {A4[0]} {A4[1]}]")
    d.set_metadata({"title": "Pages of different sizes", **INFO})
    d.save(str(OUT / "mixed-pages.pdf"), garbage=3, deflate=True, no_new_id=True)


# ---------------------------------------------------------------- 800-page book ---------------
WORDS = ("energy system hydrogen transport carbon policy demand storage grid wind solar battery "
         "emission building network model cost market supply heat efficiency capacity sector "
         "transition urban industry fuel electric vehicle data analysis scenario target").split()


def sentence(rng, n):
    w = [rng.choice(WORDS) for _ in range(n)]
    w[0] = w[0].capitalize()
    return " ".join(w) + "."


def book():
    import random
    rng = random.Random(800)
    W, H = letter
    c = canvas.Canvas(str(OUT / "book-800.pdf"), pagesize=letter, invariant=1, pageCompression=1)
    c.setTitle("Large Book Fixture")
    c.setAuthor(INFO["author"])
    c.setCreator(INFO["creator"])
    page = 0
    chapter = 0
    fig = tab = 0

    def new_page():
        nonlocal page
        if page:
            c.showPage()
        page += 1
        c.setFont("Helvetica", 8)
        c.drawRightString(W - 60, 30, str(page))

    def text_block(y, lines=6):
        for _ in range(lines):
            if y < 80:
                break
            y = para(c, 60, y, W - 120, sentence(rng, rng.randint(14, 24)) + " " + sentence(rng, rng.randint(10, 18)), size=10, leading=13)
            y -= 4
        return y

    while page < 800:
        if page % 40 == 0:
            chapter += 1
            fig = tab = 0
            new_page()
            c.setFont("Helvetica-Bold", 22)
            c.drawString(60, H - 120, f"{chapter} Chapter {chapter}")
            text_block(H - 170, 30)
            continue
        new_page()
        y = H - 70
        kind = page % 5
        if kind == 1:
            fig += 1
            y = para(c, 60, y, W - 120, f"As Figure {chapter}.{fig} shows, the {rng.choice(WORDS)} grows with {rng.choice(WORDS)} (see also Table {chapter}.{max(1, tab)}).", size=10, leading=13)
            bar_chart(c, 120, y - 230, 360, 200, [rng.randint(5, 50) for _ in range(6)], ["A", "B", "C", "D", "E", "F"])
            c.setFont("Helvetica", 9)
            c.drawString(60, y - 255, f"Figure {chapter}.{fig} {sentence(rng, 8)}")
            text_block(y - 285, 14)
        elif kind == 3:
            tab += 1
            y = para(c, 60, y, W - 120, f"Table {chapter}.{tab} provides a detailed comparison of the {rng.choice(WORDS)} options.", size=10, leading=13)
            c.setFont("Helvetica-Bold", 9)
            c.drawString(60, y - 10, f"Table {chapter}.{tab} Comparison of {rng.choice(WORDS)} and {rng.choice(WORDS)}.")
            y = table(c, 60, y - 20, [160, 100, 100, 100], [["Option", "Cost", "Share", "Change"]] + [[rng.choice(WORDS), rng.randint(10, 99), f"{rng.randint(1, 60)} %", f"+{rng.randint(1, 9)} %"] for _ in range(12)])
            text_block(y - 10, 10)
        elif kind == 4 and tab:
            c.setFont("Helvetica-Bold", 9)
            c.drawString(60, y, f"Table {chapter}.{tab} (Continued)")
            y = table(c, 60, y - 10, [160, 100, 100, 100], [["Option", "Cost", "Share", "Change"]] + [[rng.choice(WORDS), rng.randint(10, 99), f"{rng.randint(1, 60)} %", f"+{rng.randint(1, 9)} %"] for _ in range(8)])
            text_block(y - 10, 12)
        else:
            text_block(y, 40)
    c.showPage()
    c.save()


if __name__ == "__main__":
    manual()
    poster_research()
    poster_event()
    slides()
    brochure()
    report()
    rotated()
    mixed()
    book()
    print("written to", OUT)
