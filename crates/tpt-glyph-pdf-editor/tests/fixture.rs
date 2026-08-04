// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-editor integration test
//
// Loads a real fixture PDF, applies both edit operations, saves, and
// re-parses the result to confirm the round trip through the real parser
// (not just the synthetic documents used by the crate's unit tests).

use std::path::Path;
use tpt_glyph_pdf_editor::{Editor, ImageData};
use tpt_glyph_pdf_ir::{Operation, Rect};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
}

#[test]
fn edits_and_reparses_hello_fixture() {
    let path = fixtures_dir().join("pdf/hello.pdf");
    let image = ImageData {
        width: 2,
        height: 2,
        bits_per_component: 8,
        color_space: "DeviceRGB".into(),
        data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
    };

    let saved = Editor::load(&path)
        .expect("load hello.pdf")
        .replace_text(0, b"Hello", b"Goodbye TPT")
        .expect("replace_text")
        .insert_image(0, image, Rect::new(100.0, 100.0, 150.0, 150.0))
        .expect("insert_image")
        .save()
        .expect("save");

    assert!(saved.starts_with(b"%PDF-"));

    let reparsed = tpt_glyph_pdf_parser::parse_bytes(&saved).expect("re-parse edited PDF");
    assert_eq!(reparsed.page_count(), 1);
    let page = reparsed.page(0).unwrap();
    assert!(page.contents[0]
        .ops
        .contains(&Operation::TextDraw(b"Goodbye TPT".to_vec())));
    assert_eq!(page.resources.xobjects.len(), 1);
    assert!(page.contents[0]
        .ops
        .iter()
        .any(|op| matches!(op, Operation::PaintXObject(_))));
}
