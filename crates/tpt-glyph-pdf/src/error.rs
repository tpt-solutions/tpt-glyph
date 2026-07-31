// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf / error
//
// Crate error type, wrapping the underlying `pdf` crate errors and the core
// engine errors.

use thiserror::Error;

/// Errors produced while parsing or rendering a PDF document.
#[derive(Debug, Error)]
pub enum PdfError {
    #[error("pdf parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported feature: {0}")]
    Unsupported(String),

    #[error("page {0} not found (document has {1} pages)")]
    PageNotFound(usize, usize),

    #[error("empty document: no pages to render")]
    EmptyDocument,
}

impl From<pdf::error::PdfError> for PdfError {
    fn from(e: pdf::error::PdfError) -> Self {
        PdfError::Parse(e.to_string())
    }
}

/// Convenience `Result` alias for PDF operations.
pub type Result<T> = std::result::Result<T, PdfError>;
