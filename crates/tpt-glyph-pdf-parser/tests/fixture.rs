// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-parser integration test against real fixtures.

#[test]
fn fixture_hello_parses() {
    let doc = tpt_glyph_pdf_parser::parse_path("../../fixtures/pdf/hello.pdf").unwrap();
    assert_eq!(doc.page_count(), 1);
    let page = doc.page(0).unwrap();
    assert_eq!(page.width(), 200.0);
    assert!(!page.contents.is_empty());
}
