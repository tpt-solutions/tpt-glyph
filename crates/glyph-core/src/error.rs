// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-core / error
//
// Crate-wide error type and `Result` alias.

use thiserror::Error;

/// Errors produced by the glyph-core engine.
#[derive(Debug, Error)]
pub enum GlyphError {
    #[error("invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("page index out of range: {index} (document has {count} pages)")]
    PageOutOfRange { index: usize, count: usize },

    #[error("unsupported feature: {0}")]
    Unsupported(&'static str),

    #[error("graphics state stack underflow")]
    StateStackUnderflow,

    #[error("operand stack underflow")]
    OperandStackUnderflow,

    #[error("unknown operator: {0}")]
    UnknownOperator(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience `Result` alias for glyph-core operations.
pub type Result<T> = std::result::Result<T, GlyphError>;
