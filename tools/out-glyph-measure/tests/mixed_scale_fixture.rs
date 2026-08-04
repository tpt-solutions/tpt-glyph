// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — out-glyph-measure integration test
//
// Builds a real multi-page PDF (via tpt-glyph-pdf-writer) with known-length
// geometry on each page, applies a different scale per page through a
// `ScaleTable`, and confirms each page's real-world length comes out
// correct — the "mixed-scale multi-page fixture" the Phase 16 checklist
// calls for.

use out_glyph_measure::{measure_page, ScaleSpec, ScaleTable};
use tpt_glyph_pdf_writer::{Stream, Value, Writer};

/// A 2-page PDF: page 1 draws a 100-unit-long horizontal line, page 2 a
/// 50-unit-long vertical line.
fn build_fixture() -> Vec<u8> {
    let mut w = Writer::new();
    let pages_id = w.alloc();

    let page1_content = w.add_stream(Stream::new(b"0 0 m 100 0 l S".to_vec()));
    let page1 = w.add(Value::dict([
        ("Type", Value::name("Page")),
        ("Parent", Value::reference(pages_id)),
        (
            "MediaBox",
            Value::array([
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(1000),
                Value::Integer(1000),
            ]),
        ),
        ("Resources", Value::Dict(Vec::new())),
        ("Contents", Value::reference(page1_content)),
    ]));

    let page2_content = w.add_stream(Stream::new(b"0 0 m 0 50 l S".to_vec()));
    let page2 = w.add(Value::dict([
        ("Type", Value::name("Page")),
        ("Parent", Value::reference(pages_id)),
        (
            "MediaBox",
            Value::array([
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(1000),
                Value::Integer(1000),
            ]),
        ),
        ("Resources", Value::Dict(Vec::new())),
        ("Contents", Value::reference(page2_content)),
    ]));

    w.define(
        pages_id,
        Value::dict([
            ("Type", Value::name("Pages")),
            (
                "Kids",
                Value::array([Value::reference(page1), Value::reference(page2)]),
            ),
            ("Count", Value::Integer(2)),
        ]),
    )
    .unwrap();

    let catalog = w.add(Value::dict([
        ("Type", Value::name("Catalog")),
        ("Pages", Value::reference(pages_id)),
    ]));
    w.set_root(catalog);
    w.finish().unwrap()
}

#[test]
fn mixed_scale_multi_page_fixture_measures_correctly() {
    let bytes = build_fixture();
    let doc = tpt_glyph_pdf_parser::parse_bytes(&bytes).expect("parse fixture");
    assert_eq!(doc.page_count(), 2);

    let table = ScaleTable::new()
        .with_page(1, ScaleSpec::parse("1:100").unwrap())
        .with_page(2, ScaleSpec::parse("1:50").unwrap());

    // Page 1: 100 PDF units at 1:100 -> 100 * (25.4/72) * 100 mm.
    let page1 = doc.page(0).unwrap();
    let m1 = measure_page(page1, table.scale_for(1));
    assert_eq!(m1.len(), 1);
    assert!((m1[0].pdf_length - 100.0).abs() < 1e-6);
    let expected_mm1 = 100.0 * (25.4 / 72.0) * 100.0;
    assert!(
        (m1[0].real_world_mm - expected_mm1).abs() < 1e-6,
        "page 1: got {}",
        m1[0].real_world_mm
    );

    // Page 2: 50 PDF units at 1:50 -> 50 * (25.4/72) * 50 mm.
    let page2 = doc.page(1).unwrap();
    let m2 = measure_page(page2, table.scale_for(2));
    assert_eq!(m2.len(), 1);
    assert!((m2[0].pdf_length - 50.0).abs() < 1e-6);
    let expected_mm2 = 50.0 * (25.4 / 72.0) * 50.0;
    assert!(
        (m2[0].real_world_mm - expected_mm2).abs() < 1e-6,
        "page 2: got {}",
        m2[0].real_world_mm
    );

    // The two pages' scales must actually differ in effect (sanity check
    // that this test isn't accidentally using the same scale for both).
    assert!((expected_mm1 - expected_mm2).abs() > 1.0);
}

#[test]
fn missing_per_page_override_falls_back_to_table_default() {
    let bytes = build_fixture();
    let doc = tpt_glyph_pdf_parser::parse_bytes(&bytes).expect("parse fixture");

    let table = ScaleTable::new()
        .with_default(ScaleSpec::parse("1:10").unwrap())
        .with_page(2, ScaleSpec::parse("1:50").unwrap());

    // Page 1 has no override, so it should use the 1:10 default.
    let page1 = doc.page(0).unwrap();
    let m1 = measure_page(page1, table.scale_for(1));
    let expected = 100.0 * (25.4 / 72.0) * 10.0;
    assert!((m1[0].real_world_mm - expected).abs() < 1e-6);
}
