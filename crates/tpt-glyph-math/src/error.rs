// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/error
//
// Hand-rolled error type (no `thiserror`) so this module stays usable from a
// `no_std` build; `std::error::Error` is only implemented when the `std`
// feature is enabled.

use alloc::string::String;
use core::fmt;

/// Errors produced while building, laying out, or emitting a [`crate::ast::MathExpr`].
#[derive(Debug, Clone, PartialEq)]
pub enum MathError {
    /// A character has no glyph in the supplied font.
    MissingGlyph(char),
    /// A LaTeX math string could not be parsed (only constructed when the
    /// `latex-parser` feature is enabled).
    LatexParse(String),
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MathError::MissingGlyph(c) => write!(f, "no glyph for character '{c}' in font"),
            MathError::LatexParse(msg) => write!(f, "LaTeX parse error: {msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MathError {}

pub type Result<T> = core::result::Result<T, MathError>;
