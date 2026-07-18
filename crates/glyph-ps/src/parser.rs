// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-ps / parser
//
// Parser: turns the flat token stream into a tree of `PsObject` values. This is
// the "object" model the interpreter executes. Procedures (`{ ... }`) become
// executable arrays; `[ ... ]` becomes literal arrays; `/name` becomes a
// literal name; bare names become executable names.

use crate::error::{PsError, Result};
use crate::lexer::{tokenize, Token};

/// A PostScript object as produced by the parser.
#[derive(Debug, Clone, PartialEq)]
pub enum PsObject {
    /// Executable name (operator or deferred name lookup).
    Name(String),
    /// Literal name (created by `/name`).
    LiteralName(String),
    /// Integer.
    Int(i64),
    /// Real.
    Real(f64),
    /// Boolean.
    Bool(bool),
    /// Parenthesized string literal.
    Str(String),
    /// An array of objects (literal or procedure body).
    Array(Vec<PsObject>),
}

impl PsObject {
    /// Numeric value as f64, if this is an int/real.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PsObject::Int(v) => Some(*v as f64),
            PsObject::Real(v) => Some(*v),
            _ => None,
        }
    }

    /// Numeric value as i64, if this is an int.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PsObject::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            PsObject::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_executable_name(&self) -> bool {
        matches!(self, PsObject::Name(_))
    }
}

/// A parsed PostScript program: a sequence of top-level objects.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub objects: Vec<PsObject>,
}

/// Parse source text into an executable program.
pub fn parse(src: &str) -> Result<Program> {
    let tokens = tokenize(src)?;
    let mut cursor = 0usize;
    let objects = parse_sequence(&tokens, &mut cursor, None)?;
    Ok(Program { objects })
}

/// Parse a sequence of objects terminated by `end` (e.g. `ProcEnd`/`ArrayEnd`)
/// or EOF when `end` is `None`.
fn parse_sequence(
    tokens: &[Token],
    cursor: &mut usize,
    end: Option<&Token>,
) -> Result<Vec<PsObject>> {
    let mut out = Vec::new();
    while *cursor < tokens.len() {
        let tok = &tokens[*cursor];
        if let Some(e) = end {
            if tok == e {
                return Ok(out);
            }
        }
        match tok {
            Token::ProcStart => {
                *cursor += 1;
                let body = parse_sequence(tokens, cursor, Some(&Token::ProcEnd))?;
                *cursor += 1; // consume ProcEnd
                out.push(PsObject::Array(body));
            }
            Token::ArrayStart => {
                *cursor += 1;
                let body = parse_sequence(tokens, cursor, Some(&Token::ArrayEnd))?;
                *cursor += 1; // consume ArrayEnd
                out.push(PsObject::Array(body));
            }
            Token::Name(n) => {
                out.push(PsObject::Name(n.clone()));
                *cursor += 1;
            }
            Token::LiteralName(n) => {
                out.push(PsObject::LiteralName(n.clone()));
                *cursor += 1;
            }
            Token::Int(v) => {
                out.push(PsObject::Int(*v));
                *cursor += 1;
            }
            Token::Real(v) => {
                out.push(PsObject::Real(*v));
                *cursor += 1;
            }
            Token::Bool(b) => {
                out.push(PsObject::Bool(*b));
                *cursor += 1;
            }
            Token::String(s) => {
                out.push(PsObject::Str(s.clone()));
                *cursor += 1;
            }
            Token::ProcEnd | Token::ArrayEnd => {
                // Unbalanced closer with no matching opener.
                return Err(PsError::Parse(format!(
                    "unexpected closing delimiter at token {cursor}"
                )));
            }
        }
    }
    if end.is_some() {
        return Err(PsError::Parse(
            "unexpected end of input inside delimiter".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_procedure_and_array() {
        let p = parse("/x 10 def { x moveto } [1 2 3]").unwrap();
        assert_eq!(
            p.objects,
            vec![
                PsObject::LiteralName("x".into()),
                PsObject::Int(10),
                PsObject::Name("def".into()),
                PsObject::Array(vec![
                    PsObject::Name("x".into()),
                    PsObject::Name("moveto".into())
                ]),
                PsObject::Array(vec![PsObject::Int(1), PsObject::Int(2), PsObject::Int(3)]),
            ]
        );
    }

    #[test]
    fn unbalanced_procedure_errors() {
        assert!(parse("{ moveto").is_err());
        assert!(parse("moveto }").is_err());
    }

    #[test]
    fn nested_array() {
        let p = parse("[1 [2 3] 4]").unwrap();
        assert_eq!(
            p.objects,
            vec![PsObject::Array(vec![
                PsObject::Int(1),
                PsObject::Array(vec![PsObject::Int(2), PsObject::Int(3)]),
                PsObject::Int(4),
            ])]
        );
    }
}
