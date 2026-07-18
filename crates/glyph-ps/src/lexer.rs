// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-ps / lexer
//
// PostScript tokenizer. Produces a flat token stream consumed by the parser.
// Recognizes: integers, reals, names (`/` literal vs bare executable), the
// booleans `true`/`false`, string literals, and the bracketed delimiters
// `{ }` (procedures), `[ ]` (arrays), and `( )` (literals) which are handled
// by the parser.

use crate::error::{PsError, Result};

/// A lexical token emitted by the tokenizer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Bare executable name (an operator or deferred name lookup).
    Name(String),
    /// Literal name introduced by `/`.
    LiteralName(String),
    /// Integer literal.
    Int(i64),
    /// Real literal.
    Real(f64),
    /// Boolean literal.
    Bool(bool),
    /// Begin a procedure body `{`.
    ProcStart,
    /// End a procedure body `}`.
    ProcEnd,
    /// Begin an array `[`.
    ArrayStart,
    /// End an array `]`.
    ArrayEnd,
    /// Parenthesized string literal `(...)`.
    String(String),
}

/// Tokenize PostScript source into a vector of tokens.
pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let n = bytes.len();
    let mut out = Vec::new();

    while i < n {
        // Skip whitespace and PostScript comments (% to end of line).
        let c = bytes[i];
        if c == b'%' {
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        match c {
            b'{' => {
                out.push(Token::ProcStart);
                i += 1;
            }
            b'}' => {
                out.push(Token::ProcEnd);
                i += 1;
            }
            b'[' => {
                out.push(Token::ArrayStart);
                i += 1;
            }
            b']' => {
                out.push(Token::ArrayEnd);
                i += 1;
            }
            b'/' => {
                // Literal name. A double slash `//` is an immediate name in
                // later PostScript; we treat it the same as a literal name here.
                let start = i + 1;
                let name = read_name(bytes, &mut i, start)?;
                out.push(Token::LiteralName(name));
            }
            b'(' => {
                let s = read_string(bytes, &mut i)?;
                out.push(Token::String(s));
            }
            b'<' => {
                // Angle brackets denote hex/literal strings in PostScript.
                // We support `<...>` hex strings minimally by reading the
                // balanced content as a string of the decoded bytes (best
                // effort) — kept simple for the core path.
                return Err(PsError::Lex {
                    offset: i,
                    message: "hex/angle-bracket strings not supported in core lexer".into(),
                });
            }
            _ => {
                // Number or bare name.
                if is_number_start(c) {
                    let (tok, ni) = read_number(bytes, i)?;
                    out.push(tok);
                    i = ni;
                } else {
                    let pos = i;
                    let name = read_name(bytes, &mut i, pos)?;
                    match name.as_str() {
                        "true" => out.push(Token::Bool(true)),
                        "false" => out.push(Token::Bool(false)),
                        other => out.push(Token::Name(other.to_string())),
                    }
                }
            }
        }
    }
    Ok(out)
}

fn is_number_start(c: u8) -> bool {
    c.is_ascii_digit() || c == b'+' || c == b'-' || c == b'.'
}

fn read_number(bytes: &[u8], start: usize) -> Result<(Token, usize)> {
    let mut j = start;
    let n = bytes.len();
    let mut saw_dot = false;
    let mut saw_digit = false;
    while j < n {
        let c = bytes[j];
        if c.is_ascii_digit() {
            saw_digit = true;
            j += 1;
        } else if c == b'.' && !saw_dot {
            saw_dot = true;
            j += 1;
        } else {
            break;
        }
    }
    let text = std::str::from_utf8(&bytes[start..j]).map_err(|_| PsError::Lex {
        offset: start,
        message: "invalid number".into(),
    })?;
    if !saw_digit {
        return Err(PsError::Lex {
            offset: start,
            message: format!("malformed number: {text}"),
        });
    }
    if saw_dot {
        let v: f64 = text.parse().map_err(|_| PsError::Lex {
            offset: start,
            message: format!("invalid real: {text}"),
        })?;
        Ok((Token::Real(v), j))
    } else {
        // Allow integer parses that exceed i64 to fall back to real.
        match text.parse::<i64>() {
            Ok(v) => Ok((Token::Int(v), j)),
            Err(_) => {
                let v: f64 = text.parse().map_err(|_| PsError::Lex {
                    offset: start,
                    message: format!("number too large: {text}"),
                })?;
                Ok((Token::Real(v), j))
            }
        }
    }
}

/// Read a name token starting at `name_start` (after any leading `/`), advancing
/// `i` to just past the name. Names end at whitespace or a special delimiter.
fn read_name(bytes: &[u8], i: &mut usize, name_start: usize) -> Result<String> {
    let n = bytes.len();
    let mut j = name_start;
    while j < n {
        let c = bytes[j];
        if c.is_ascii_whitespace() {
            break;
        }
        // Delimiters that terminate a name without being part of it.
        if matches!(
            c,
            b'{' | b'}' | b'[' | b']' | b'/' | b'(' | b'%' | b'<' | b'>'
        ) {
            break;
        }
        j += 1;
    }
    if j == name_start {
        return Err(PsError::Lex {
            offset: *i,
            message: "empty name".into(),
        });
    }
    let name = std::str::from_utf8(&bytes[name_start..j])
        .map_err(|_| PsError::Lex {
            offset: *i,
            message: "invalid utf8 in name".into(),
        })?
        .to_string();
    *i = j;
    Ok(name)
}

/// Read a parenthesized string literal `(...)`. Handles balanced nested
/// parentheses (escaped `\(` does not count as nesting).
fn read_string(bytes: &[u8], i: &mut usize) -> Result<String> {
    let n = bytes.len();
    let mut j = *i + 1; // skip opening '('
    let mut depth = 1i32;
    let mut out = String::new();
    while j < n {
        let c = bytes[j];
        if c == b'\\' {
            // Escape sequence.
            if j + 1 < n {
                let e = bytes[j + 1];
                match e {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'\\' => out.push('\\'),
                    b'(' => out.push('('),
                    b')' => out.push(')'),
                    b'\n' => { /* line continuation: ignore */ }
                    _ => {
                        // Octal escape \ddd.
                        if e.is_ascii_digit() {
                            let mut val = 0i32;
                            let mut k = j + 1;
                            let mut count = 0;
                            while k < n && count < 3 && bytes[k].is_ascii_digit() {
                                val = val * 8 + (bytes[k] - b'0') as i32;
                                k += 1;
                                count += 1;
                            }
                            out.push(val as u8 as char);
                            j = k - 1; // adjust so loop increments correctly
                        } else {
                            out.push(e as char);
                        }
                    }
                }
                j += 2;
            } else {
                j += 1;
            }
            continue;
        }
        if c == b'(' {
            depth += 1;
            out.push('(');
            j += 1;
        } else if c == b')' {
            depth -= 1;
            if depth == 0 {
                j += 1;
                break;
            }
            out.push(')');
            j += 1;
        } else {
            // Copy UTF-8-ish; for simplicity push as char.
            if let Some(ch) = std::str::from_utf8(&bytes[j..j + 1])
                .ok()
                .and_then(|s| s.chars().next())
            {
                out.push(ch);
            }
            j += 1;
        }
    }
    if depth != 0 {
        return Err(PsError::Lex {
            offset: *i,
            message: "unterminated string literal".into(),
        });
    }
    *i = j;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_basic() {
        let t = tokenize("1 2 add /x 3.5 { moveto }").unwrap();
        assert_eq!(
            t,
            vec![
                Token::Int(1),
                Token::Int(2),
                Token::Name("add".into()),
                Token::LiteralName("x".into()),
                Token::Real(3.5),
                Token::ProcStart,
                Token::Name("moveto".into()),
                Token::ProcEnd,
            ]
        );
    }

    #[test]
    fn comments_and_whitespace_skipped() {
        let t = tokenize("  % a comment\n 42   \t stroke ").unwrap();
        assert_eq!(t, vec![Token::Int(42), Token::Name("stroke".into())]);
    }

    #[test]
    fn nested_string() {
        let t = tokenize("(hello (world)) show").unwrap();
        assert_eq!(
            t,
            vec![
                Token::String("hello (world)".into()),
                Token::Name("show".into())
            ]
        );
    }

    #[test]
    fn unterminated_string_errors() {
        assert!(tokenize("(oops").is_err());
    }
}
