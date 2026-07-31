// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf / document
//
// Document-level PDF handling: open a file, walk the page tree, and expose
// per-page metadata (dimensions, decoded content ops, resources). Rendering is
// delegated to the content-stream interpreter in `content.rs`.

use crate::content::{render_ops, TextState};
use crate::error::{PdfError, Result};
use tpt_glyph_core::canvas::Canvas;
use tpt_glyph_core::document::Page;
use tpt_glyph_core::graphics_state::GraphicsState;
use tpt_glyph_core::render::{DebugRasterizer, Rasterizer, RenderTree};
use pdf::file::{File, FileOptions, NoCache, NoLog};
use pdf::object::Resolve;
use pdf::object::*;

/// Concrete `File` type used by this crate: in-memory backend with no caches.
type PdfFile = File<Vec<u8>, NoCache, NoCache, NoLog>;

/// A parsed PDF document, backed by an in-memory byte buffer.
pub struct PdfDocument {
    file: PdfFile,
    page_count: usize,
}

impl PdfDocument {
    /// Open and parse a PDF from an in-memory buffer.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let file = FileOptions::uncached().load(data)?;
        let page_count = file.num_pages() as usize;
        Ok(Self { file, page_count })
    }

    /// Open and parse a PDF from a file path.
    pub fn open_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = FileOptions::uncached().open(path)?;
        let page_count = file.num_pages() as usize;
        Ok(Self { file, page_count })
    }

    /// Number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Collect metadata for every page (dimensions + decoded content ops).
    pub fn pages(&self) -> Result<Vec<PdfPageInfo>> {
        let resolver = self.file.resolver();
        let mut out = Vec::with_capacity(self.page_count);
        for idx in 0..self.page_count {
            let page_rc = self.file.get_page(idx as u32)?;
            let page = &*page_rc; // PageRc derefs to Page
            let rect = page.media_box()?;
            let width = (rect.right - rect.left).round().max(1.0) as u32;
            let height = (rect.top - rect.bottom).round().max(1.0) as u32;

            let ops = match page.contents {
                Some(ref content) => content.operations(&resolver)?,
                None => Vec::new(),
            };
            let resources = page
                .resources
                .as_ref()
                .map(|r| (**r).clone())
                .unwrap_or_default();

            out.push(PdfPageInfo {
                index: idx,
                width,
                height,
                ops,
                resources,
            });
        }
        Ok(out)
    }
}

/// Per-page information extracted from a PDF: device dimensions and the decoded
/// sequence of content-stream operators.
pub struct PdfPageInfo {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub ops: Vec<pdf::content::Op>,
    pub resources: Resources,
}

impl PdfPageInfo {
    /// Render this page into a `RenderTree` using `state` as the initial
    /// graphics state. `resolver` resolves indirectly-referenced fonts/XObjects.
    pub fn render(&self, state: GraphicsState, resolver: &impl Resolve) -> RenderTree {
        let mut tree = RenderTree::new(self.width, self.height);
        let mut gstate = state;
        let mut tstate = TextState::default();
        let mut path = tpt_glyph_core::geometry::Path::new();
        render_ops(
            &self.ops,
            &self.resources,
            resolver,
            &mut gstate,
            &mut tstate,
            &mut path,
            &mut tree,
        );
        tree
    }
}

/// Render a single page of an already-parsed document to a `Canvas`.
pub fn render_page(doc: &PdfDocument, index: usize, state: GraphicsState) -> Result<Canvas> {
    let pages = doc.pages()?;
    let resolver = doc.file.resolver();
    let info = pages
        .into_iter()
        .find(|p| p.index == index)
        .ok_or_else(|| PdfError::PageNotFound(index, doc.page_count()))?;
    let tree = info.render(state, &resolver);
    DebugRasterizer
        .rasterize(&tree)
        .map_err(|e| PdfError::Parse(format!("rasterize error: {e}")))
}

/// Render every page of a document into a `Document`.
pub fn render_document(doc: &PdfDocument) -> Result<tpt_glyph_core::document::Document> {
    if doc.page_count() == 0 {
        return Err(PdfError::EmptyDocument);
    }
    let pages = doc.pages()?;
    let resolver = doc.file.resolver();
    let pages: Vec<Page> = pages
        .into_iter()
        .map(|info| {
            let tree = info.render(GraphicsState::new(), &resolver);
            let canvas: Canvas = DebugRasterizer.rasterize(&tree).expect("rasterize");
            Page::new(info.index, canvas)
        })
        .collect();
    Ok(tpt_glyph_core::document::Document { pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but spec-valid one-page PDF (correct xref offsets) with a
    /// filled rectangle, a stroked line, and a text show. Used to exercise the
    /// parser → content-stream → pipeline path.
    fn sample_pdf() -> Vec<u8> {
        // Stream content for the page (no leading/trailing EOL beyond the single
        // LF separating operators; `endstream` follows immediately).
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

    #[test]
    fn parses_page_tree_and_dimensions() {
        let doc = PdfDocument::from_bytes(sample_pdf()).unwrap();
        assert_eq!(doc.page_count(), 1);
        let pages = doc.pages().unwrap();
        let p = &pages[0];
        assert_eq!(p.index, 0);
        assert_eq!(p.width, 200);
        assert_eq!(p.height, 200);
        // The content stream should have produced several operators.
        assert!(p.ops.len() >= 8, "expected many ops, got {}", p.ops.len());
    }

    #[test]
    fn renders_non_empty_tree() {
        let doc = PdfDocument::from_bytes(sample_pdf()).unwrap();
        let canvas = render_page(&doc, 0, GraphicsState::new()).unwrap();
        assert_eq!(canvas.len(), 200 * 200);
        // The filled rectangle + stroked line + text boxes must produce output.
        assert!(canvas
            .pixels
            .iter()
            .any(|px| px.r > 0 || px.g > 0 || px.b > 0));
    }

    #[test]
    fn empty_document_errors() {
        // A valid PDF whose /Pages tree has zero kids.
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [] /Count 0 >>".into(),
        ];
        let mut body = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            let num = i + 1;
            offsets.push(body.len());
            body.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
            body.extend_from_slice(obj.as_bytes());
            body.extend_from_slice(b"\nendobj\n");
        }
        let xref_start = body.len();
        let mut xref = String::from("xref\n0 3\n0000000000 65535 f \n");
        for off in &offsets {
            xref.push_str(&format!("{off:010} 00000 n \n"));
        }
        body.extend_from_slice(xref.as_bytes());
        body.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 3 >>\nstartxref\n{xref_start}\n%%EOF")
                .as_bytes(),
        );
        let doc = PdfDocument::from_bytes(body).unwrap();
        assert_eq!(doc.page_count(), 0);
        assert!(render_document(&doc).is_err());
    }
}
