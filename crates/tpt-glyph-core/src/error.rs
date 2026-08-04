// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / error
//
// Crate-wide error type and `Result` alias.
//
// Implemented by hand (rather than via `thiserror`) so the crate stays `#![no_std]`
// compatible: `thiserror`'s derive unconditionally emits `impl std::error::Error`,
// which cannot compile in a no_std build. The `std::error::Error` impl and the
// `Io` variant are therefore gated behind the `std` feature.

use alloc::string::String;
use core::fmt;

/// Errors produced by the tpt-glyph-core engine.
#[derive(Debug)]
pub enum GlyphError {
    InvalidDimensions {
        width: u32,
        height: u32,
    },
    PageOutOfRange {
        index: usize,
        count: usize,
    },
    Unsupported(&'static str),
    StateStackUnderflow,
    OperandStackUnderflow,
    UnknownOperator(String),
    Parse(String),
    ResourceLimit(String),

    #[cfg(feature = "std")]
    Io(std::io::Error),
}

impl fmt::Display for GlyphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlyphError::InvalidDimensions { width, height } => {
                write!(f, "invalid dimensions: {width}x{height}")
            }
            GlyphError::PageOutOfRange { index, count } => {
                write!(
                    f,
                    "page index out of range: {index} (document has {count} pages)"
                )
            }
            GlyphError::Unsupported(msg) => write!(f, "unsupported feature: {msg}"),
            GlyphError::StateStackUnderflow => write!(f, "graphics state stack underflow"),
            GlyphError::OperandStackUnderflow => write!(f, "operand stack underflow"),
            GlyphError::UnknownOperator(op) => write!(f, "unknown operator: {op}"),
            GlyphError::Parse(msg) => write!(f, "parse error: {msg}"),
            GlyphError::ResourceLimit(msg) => write!(f, "resource limit exceeded: {msg}"),
            #[cfg(feature = "std")]
            GlyphError::Io(err) => write!(f, "{err}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GlyphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GlyphError::Io(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for GlyphError {
    fn from(err: std::io::Error) -> Self {
        GlyphError::Io(err)
    }
}

/// Convenience `Result` alias for tpt-glyph-core operations.
pub type Result<T> = core::result::Result<T, GlyphError>;
