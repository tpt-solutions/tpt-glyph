// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — fuzz target: PDF content-stream path.
//
// Fuzzes the PDF rendering path with adversarial input. We render a single
// synthetic page whose content stream is the fuzz bytes; this exercises the
// `pdf` crate decoder, the tpt-glyph-pdf content interpreter, and the rasterizer.
// Run with: `cargo +nightly fuzz run pdf_content`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Build a minimal one-page PDF wrapping the fuzz bytes as a content stream.
    // If the bytes aren't valid PDF operators, the decoder should error cleanly
    // rather than crash.
    let content = String::from_utf8_lossy(data);
    let body = format!(
        "%PDF-1.4\n1 0 obj<<</Type/Catalog/Pages 2 0 R>>endobj\n\
         2 0 obj<<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
         3 0 obj<<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Contents 4 0 R>>endobj\n\
         4 0 obj<<</Length {}>>\nstream\n{}\nendstream\nendobj\n\
         xref\n0 5\n0000000000 65535 f \n\
         0000000009 00000 n \n0000000052 00000 n \n0000000101 00000 n \n\
         0000000190 00000 n \ntrailer<<</Size 5/Root 1 0 R>>\nstartxref\n260\n%%EOF\n",
        content.len(),
        content
    );
    let doc = match tpt_glyph_pdf::PdfDocument::from_bytes(body.into_bytes()) {
        Ok(d) => d,
        Err(_) => return,
    };
    if doc.page_count() == 0 {
        return;
    }
    // Rasterize must not panic/hang/OOM on adversarial content.
    let _ = tpt_glyph_pdf::render_page(&doc, 0, tpt_glyph_core::graphics_state::GraphicsState::new());
});
