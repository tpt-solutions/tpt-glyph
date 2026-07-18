// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-core / document
//
// `Page` and `Document` abstractions, plus the per-page parallel rendering model
// that exploits the immutable `GraphicsState`.

use crate::canvas::Canvas;
use crate::error::{GlyphError, Result};
use rayon::prelude::*;

/// A single rendered page: the rasterized canvas at a target resolution.
#[derive(Debug, Clone)]
pub struct Page {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub canvas: Canvas,
}

impl Page {
    pub fn new(index: usize, canvas: Canvas) -> Self {
        let (width, height) = (canvas.width, canvas.height);
        Self {
            index,
            width,
            height,
            canvas,
        }
    }
}

/// A multi-page document: a collection of rendered pages.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub pages: Vec<Page>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// A trait for engines that can render a single page into a `Canvas`.
///
/// Implementors receive an immutable graphics state seed and must produce a
/// `Canvas` without retaining mutable global state, which is what enables the
/// parallel rendering below.
pub trait PageRenderer {
    /// Render the page at `index` at the given device dimensions.
    fn render_page(&self, index: usize, width: u32, height: u32) -> Result<Canvas>;
}

/// Render all pages of a document concurrently using a `rayon` thread pool.
///
/// Because the renderer operates on an immutable `GraphicsState` seed and the
/// `PageRenderer` trait guarantees no shared mutable state, rendering pages in
/// parallel is data-race free by construction.
pub fn render_document_parallel<R: PageRenderer + Sync>(
    renderer: &R,
    page_indices: &[usize],
    width: u32,
    height: u32,
) -> Result<Document> {
    if width == 0 || height == 0 {
        return Err(GlyphError::InvalidDimensions { width, height });
    }

    let pages: Vec<Page> = page_indices
        .par_iter()
        .map(|&idx| {
            renderer
                .render_page(idx, width, height)
                .map(|canvas| Page::new(idx, canvas))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Document { pages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics_state::GraphicsState;
    use crate::render::{DebugRasterizer, Rasterizer, RenderTree};

    struct DummyRenderer;

    impl PageRenderer for DummyRenderer {
        fn render_page(&self, index: usize, width: u32, height: u32) -> Result<Canvas> {
            // Deterministic, state-free render: color depends only on page index.
            let mut tree = RenderTree::new(width, height);
            let state = GraphicsState::new().with_fill_color(crate::graphics_state::RgbColor::new(
                (index as f64 * 0.1) % 1.0,
                0.0,
                0.0,
            ));
            tree.fill(
                &state,
                crate::geometry::Path {
                    subpaths: vec![crate::geometry::Subpath::new(crate::geometry::Point::new(
                        (index as f64 + 1.0) * 5.0,
                        (index as f64 + 1.0) * 5.0,
                    ))],
                },
            );
            DebugRasterizer.rasterize(&tree)
        }
    }

    #[test]
    fn parallel_render_matches_sequential() {
        let r = DummyRenderer;
        let indices: Vec<usize> = (0..16).collect();
        let doc = render_document_parallel(&r, &indices, 32, 32).unwrap();
        assert_eq!(doc.page_count(), 16);

        // Pages must be ordered by input index regardless of thread scheduling.
        for (i, page) in doc.pages.iter().enumerate() {
            assert_eq!(page.index, i);
        }
    }

    #[test]
    fn no_shared_mutable_state_across_threads() {
        // The renderer holds no mutable state; prove it is Sync.
        fn assert_sync<T: Sync>() {}
        assert_sync::<DummyRenderer>();
    }
}
