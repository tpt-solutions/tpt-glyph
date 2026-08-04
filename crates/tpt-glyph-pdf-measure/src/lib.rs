// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-measure
//
// Geometry and text-metric measurement over the tpt-glyph-pdf-ir document
// model: geometric bounding boxes and path lengths of painted (filled/
// stroked) path geometry, an ink-coverage estimate, and font-based text
// metrics (advance widths, ascent/descent). This crate is deliberately
// rendering-agnostic — it walks content-stream operators directly into
// geometry, without rasterizing — so it stays usable by measurement tools
// (`tools/tpt-glyph-measure`) that only need PDF-unit geometry, not pixels.

//! # tpt-glyph-pdf-measure
//!
//! ```
//! use tpt_glyph_pdf_ir::{ContentStream, Matrix, Operation, Page, Rect, Resources};
//! use tpt_glyph_pdf_measure::geometry::{bounding_box, painted_paths};
//!
//! let page = Page {
//!     index: 0,
//!     label: None,
//!     media_box: Rect::new(0.0, 0.0, 200.0, 200.0),
//!     crop_box: None,
//!     bleed_box: None,
//!     trim_box: None,
//!     art_box: None,
//!     rotate: 0,
//!     resources: Resources::empty(),
//!     contents: vec![ContentStream::new(vec![
//!         Operation::Rectangle(10.0, 10.0, 80.0, 60.0),
//!         Operation::Fill,
//!     ])],
//!     annotations: Vec::new(),
//!     thumb: None,
//!     struct_parents: None,
//! };
//! let paths = painted_paths(&page);
//! let bbox = bounding_box(&paths).unwrap();
//! assert!((bbox.width() - 80.0).abs() < 1e-6);
//! assert!((bbox.height() - 60.0).abs() < 1e-6);
//! # let _ = Matrix::IDENTITY;
//! ```

pub mod geometry;
pub mod text;

pub use geometry::{
    bounding_box, ink_coverage, painted_paths, total_length, PaintKind, PaintedPath,
};
pub use text::{measure_text, FontMetrics, TextMetrics};
