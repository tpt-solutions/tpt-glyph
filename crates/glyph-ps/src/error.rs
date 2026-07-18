// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-ps / error
//
// Crate-wide error type for the PostScript interpreter. Reuses the core
// `GlyphError` categories where sensible and adds interpreter-specific ones.

use thiserror::Error;

/// Errors produced by the PostScript interpreter.
#[derive(Debug, Error)]
pub enum PsError {
    #[error("tokenizer error at offset {offset}: {message}")]
    Lex { offset: usize, message: String },

    #[error("parse error: {0}")]
    Parse(String),

    #[error("operand stack underflow")]
    OperandStackUnderflow,

    #[error("graphics-state stack underflow")]
    StateStackUnderflow,

    #[error("execution stack underflow")]
    ExecStackUnderflow,

    #[error("type error: expected {expected}, found {found}")]
    TypeError {
        expected: &'static str,
        found: &'static str,
    },

    #[error("range error in {operator}: {value} is out of the valid set")]
    Range { operator: &'static str, value: f64 },

    #[error("unknown operator: {0}")]
    UnknownOperator(String),

    #[error("dictionary error: {0}")]
    Dict(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<PsError> for glyph_core::error::GlyphError {
    fn from(e: PsError) -> Self {
        match e {
            PsError::OperandStackUnderflow => glyph_core::error::GlyphError::OperandStackUnderflow,
            PsError::StateStackUnderflow => glyph_core::error::GlyphError::StateStackUnderflow,
            PsError::UnknownOperator(name) => glyph_core::error::GlyphError::UnknownOperator(name),
            PsError::Parse(m) => glyph_core::error::GlyphError::Parse(m),
            PsError::Lex { message, .. } => glyph_core::error::GlyphError::Parse(message),
            PsError::TypeError { expected, found } => glyph_core::error::GlyphError::Parse(
                format!("type error: expected {expected}, found {found}"),
            ),
            PsError::Range { operator, value } => {
                glyph_core::error::GlyphError::Parse(format!("range error in {operator}: {value}"))
            }
            PsError::Dict(m) => glyph_core::error::GlyphError::Parse(m),
            PsError::ExecStackUnderflow => {
                glyph_core::error::GlyphError::Parse("execution stack underflow".into())
            }
            PsError::Io(e) => glyph_core::error::GlyphError::Io(e),
        }
    }
}

/// Convenience `Result` alias for interpreter operations.
pub type Result<T> = std::result::Result<T, PsError>;
