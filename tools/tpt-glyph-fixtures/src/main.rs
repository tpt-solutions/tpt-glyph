// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tools / tpt-glyph-fixtures
//
// Generates synthetic multi-page PDF fixtures used by the Phase 7 concurrent
// rendering stress tests. The PDFs are hand-assembled (no writer dependency) so
// the fixture corpus stays self-contained and reproducible.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("pdf");
    std::fs::create_dir_all(&out_dir)?;

    // A 4-page document: each page draws a different-sized, different-colored
    // rectangle so that a race/cross-page-leak bug would surface as a page
    // picking up the wrong color or geometry.
    let pages = 4;
    let path = out_dir.join(format!("multipage-{pages}.pdf"));
    let bytes = build_multi_page_pdf(pages);
    let mut f = File::create(&path)?;
    f.write_all(&bytes)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn build_multi_page_pdf(pages: usize) -> Vec<u8> {
    // Object plan:
    // 1: Catalog
    // 2: Pages tree
    // for each page i:
    //   2*i+1 : Page object   -> /Parent 2 0 R, /Contents (2*i+2) 0 R, /MediaBox
    //   2*i+2 : Content stream
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());

    // Pages object is object 2; its /Kids reference page objects 3,5,7,...
    let mut kids = String::new();
    for i in 0..pages {
        kids.push_str(&format!("{} 0 R ", 2 * i + 3));
    }
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids, pages).into_bytes());

    for i in 0..pages {
        let size = 200 + i * 80;
        let r = (i % 3) as f64 * 0.3;
        let g = 0.4;
        let b = 0.2;
        let content = format!(
            "72 72 {} {} re {:.2} {:.2} {:.2} rg f\n",
            size, size, r, g, b
        );
        let content_bytes = content.into_bytes();
        // Page object
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R >>",
                2 * i + 4
            )
            .into_bytes(),
        );
        // Content stream object
        objects.push(stream_object(&content_bytes));
    }

    // Serialize with a proper xref.
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (idx, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        let header = format!("{} 0 obj\n", idx + 1);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = out.len();
    let n = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {}\n", n).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n", n).as_bytes(),
    );
    out.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

fn stream_object(content: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(format!("<< /Length {} >>\nstream\n", content.len()).as_bytes());
    v.extend_from_slice(content);
    v.extend_from_slice(b"\nendstream");
    v
}
