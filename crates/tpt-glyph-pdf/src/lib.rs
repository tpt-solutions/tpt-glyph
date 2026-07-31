// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf
//
// PDF parsing and rendering. Opens a PDF with the `pdf` crate, walks the page
// tree, and maps each content-stream operator (`pdf::content::Op`) onto the
// shared tpt-glyph-core rendering pipeline.

pub mod content;
pub mod document;
pub mod error;
pub mod fonts;
pub mod xobject;

pub use document::{render_document, render_page, PdfDocument, PdfPageInfo};
pub use error::{PdfError, Result};
