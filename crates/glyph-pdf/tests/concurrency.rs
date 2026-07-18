// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-pdf / stress & concurrency tests
//
// Phase 7: prove that per-page rendering is safe to run concurrently across a
// rayon thread pool and that no graphics state leaks between pages. Each page
// in the synthetic `multipage-4.pdf` fixture draws a distinct colored rectangle,
// so a cross-page state-leak bug would surface as a page picking up the wrong
// color/geometry, or as non-deterministic output across repeated parallel runs.

use glyph_core::canvas::Canvas;
use glyph_core::graphics_state::GraphicsState;
use glyph_pdf::PdfDocument;
use rayon::prelude::*;

/// Locate the `multipage-4.pdf` fixture shipped in the repo's fixture corpus.
fn fixture_path() -> std::path::PathBuf {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("pdf")
        .join("multipage-4.pdf");
    assert!(base.exists(), "missing fixture: {}", base.display());
    base
}

/// The dominant (most common non-white) color of a canvas.
fn dominant_color(canvas: &Canvas) -> (u8, u8, u8) {
    use std::collections::HashMap;
    let mut counts: HashMap<(u8, u8, u8), usize> = HashMap::new();
    for p in &canvas.pixels {
        if p.r == 255 && p.g == 255 && p.b == 255 {
            continue;
        }
        *counts.entry((p.r, p.g, p.b)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k)
        .unwrap_or((255, 255, 255))
}

#[test]
fn renders_all_pages_sequentially() {
    let doc = PdfDocument::open_path(fixture_path()).unwrap();
    assert_eq!(doc.page_count(), 4);
    let mut last: Option<(u8, u8, u8)> = None;
    for i in 0..doc.page_count() {
        let canvas = glyph_pdf::render_page(&doc, i, GraphicsState::new()).unwrap();
        let color = dominant_color(&canvas);
        // Every page must render a visible (non-white) colored rectangle.
        assert!(color != (255, 255, 255), "page {i} rendered empty/white");
        // Pages 0, 1, 2 must be pairwise distinct by construction; page 3 reuses
        // page 0's color on purpose, so only compare consecutive pages for 1..=2.
        if let Some(prev) = last {
            if i <= 2 {
                assert_ne!(color, prev, "page {i} indistinguishable from previous");
            }
        }
        last = Some(color);
    }
}

#[test]
fn concurrent_render_is_deterministic_and_leak_free() {
    let doc = PdfDocument::open_path(fixture_path()).unwrap();
    let count = doc.page_count();
    assert_eq!(count, 4);

    // Render every page many times across the thread pool. A data race or
    // shared mutable state would make repeated renders of the same page
    // disagree, or pages would bleed into each other.
    let runs = 16;
    let results: Vec<Vec<(u8, u8, u8)>> = (0..runs)
        .into_par_iter()
        .map(|_| {
            (0..count)
                .map(|i| {
                    let canvas = glyph_pdf::render_page(&doc, i, GraphicsState::new()).unwrap();
                    dominant_color(&canvas)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Every run must produce identical per-page colors: proof of determinism
    // and absence of cross-page state leakage under concurrency.
    let first = &results[0];
    for (run_idx, run) in results.iter().enumerate() {
        assert_eq!(run, first, "parallel run {run_idx} diverged from baseline");
    }

    // Pages 0 and 2 differ (distinct fixtures), confirming each page is rendered
    // from its own content rather than a shared/global state.
    assert_ne!(first[0], first[2], "pages should be distinguishable");
}
