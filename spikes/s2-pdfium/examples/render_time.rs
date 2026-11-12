//! S1 follow-up: how long does PDFium take to render the pages that freeze PDF.js?
//! usage: cargo run --release --example render_time -- <pdfium-dir> <scale> <pdf> <page>...
//! (scale = PDF.js page scale × device pixel ratio, e.g. 2 × 1.75 = 3.5)

use std::time::Instant;

use pdfium_render::prelude::*;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(a.len() >= 4, "usage: render_time <pdfium-dir> <scale> <pdf> <page>...");
    let pdfium = Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&a[0]))?);
    let scale: f32 = a[1].parse()?;
    let doc = pdfium.load_pdf_from_file(&a[2], None)?;
    for p in &a[3..] {
        let idx: i32 = p.parse::<i32>()? - 1;
        let t = Instant::now();
        let page = doc.pages().get(idx)?;
        let load = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let cfg = PdfRenderConfig::new().scale_page_by_factor(scale).render_form_data(true);
        let bmp = page.render_with_config(&cfg)?;
        let render = t.elapsed().as_secs_f64() * 1000.0;
        let bytes = bmp.as_raw_bytes().len();
        println!(
            "page {p}: load {load:.0} ms, render {render:.0} ms, {}x{} px ({} MB raw)",
            bmp.width(),
            bmp.height(),
            bytes / 1_048_576
        );
    }
    Ok(())
}
