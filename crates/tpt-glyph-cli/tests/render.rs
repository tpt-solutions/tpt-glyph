// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-cli integration tests
//
// Drives the library crates (tpt-glyph-ps, tpt-glyph-pdf) the same way the CLI does and
// asserts that real fixture inputs produce non-empty, correctly-dimensioned
// raster output. These are end-to-end "does it render" checks; pixel fidelity is
// covered by the Phase 1 visual-diff harness.

use std::path::Path;
use tpt_glyph_core::canvas::Canvas;
use tpt_glyph_core::graphics_state::GraphicsState;
use tpt_glyph_core::render::{DebugRasterizer, Rasterizer};
use tpt_glyph_pdf::PdfDocument;
use tpt_glyph_ps::Interpreter;

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
}

fn non_empty(canvas: &Canvas) {
    assert!(!canvas.is_empty(), "canvas must have pixels");
    assert!(
        canvas.pixels.iter().any(|p| p.r > 0 || p.g > 0 || p.b > 0),
        "canvas must not be blank"
    );
}

#[test]
fn cli_renders_postscript_fixture() {
    let ps = fixtures_dir().join("ps/shapes.ps");
    let src = std::fs::read_to_string(&ps).expect("read shapes.ps");
    let mut interp = Interpreter::new(612, 792);
    interp.run_source(&src).expect("interpreter run");
    let canvas = DebugRasterizer.rasterize(interp.tree()).expect("rasterize");
    assert_eq!((canvas.width, canvas.height), (612, 792));
    non_empty(&canvas);
}

#[test]
fn cli_renders_pdf_fixture() {
    let pdf = fixtures_dir().join("pdf/hello.pdf");
    let doc = PdfDocument::open_path(&pdf).expect("open pdf");
    assert_eq!(doc.page_count(), 1);
    let canvas = tpt_glyph_pdf::render_page(&doc, 0, GraphicsState::new()).expect("render page");
    assert_eq!((canvas.width, canvas.height), (200, 200));
    non_empty(&canvas);
}

#[test]
fn cli_page_range_resolves() {
    // A 1-page doc: "1" -> [0], "2" (out of range) -> empty.
    assert_eq!(resolve(&["1"]), vec![0usize]);
    assert!(resolve(&["2"]).is_empty());
    assert_eq!(resolve(&["1-1"]), vec![0usize]);
}

// Replicates the CLI's page-range parser for a self-contained unit check.
fn resolve(parts: &[&str]) -> Vec<usize> {
    let total = 1usize;
    let mut out = Vec::new();
    for part in parts {
        if let Some((a, b)) = part.split_once('-') {
            let a: usize = a.parse().unwrap();
            let b: usize = b.parse().unwrap();
            for p in a..=b {
                if (1..=total).contains(&p) {
                    out.push(p - 1);
                }
            }
        } else {
            let p: usize = part.parse().unwrap();
            if (1..=total).contains(&p) {
                out.push(p - 1);
            }
        }
    }
    out
}
