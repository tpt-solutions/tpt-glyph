// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf example
//
// Generates the sample PDF fixture (`fixtures/pdf/hello.pdf`) used by the
// visual-diff harness and by `tpt-glyph-pdf` tests. Run with:
//   cargo run -p tpt-glyph-pdf --example gen_fixture

use tpt_glyph_pdf::document::PdfDocument;

fn main() {
    let bytes = sample_pdf();
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/pdf/hello.pdf".into());
    std::fs::write(&path, &bytes).expect("write fixture");
    let doc = PdfDocument::from_bytes(bytes).expect("fixture must parse");
    println!("wrote {} ({} pages)", path, doc.page_count());
}

/// Build a minimal but spec-valid one-page PDF (correct xref offsets) with a
/// filled rectangle, a stroked line, and a text show.
fn sample_pdf() -> Vec<u8> {
    let stream = "0.2 0.4 0.8 rg\n10 10 80 60 re\nf\n0 0 0 RG\n2 w\n10 10 m\n190 190 l\nS\nBT /F1 12 Tf 1 0 0 1 20 150 Tm (Hello) Tj ET\n";
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".into(),
        format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];

    let mut body = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        let num = i + 1;
        offsets.push(body.len());
        body.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        body.extend_from_slice(obj.as_bytes());
        body.extend_from_slice(b"\nendobj\n");
    }

    let xref_start = body.len();
    let mut xref = String::from("xref\n");
    xref.push_str(&format!("0 {}\n", objects.len() + 1));
    xref.push_str("0000000000 65535 f \n");
    for off in &offsets {
        xref.push_str(&format!("{off:010} 00000 n \n"));
    }
    body.extend_from_slice(xref.as_bytes());
    body.extend_from_slice(
        format!(
            "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{}\n%%EOF",
            objects.len() + 1,
            xref_start
        )
        .as_bytes(),
    );
    body
}
