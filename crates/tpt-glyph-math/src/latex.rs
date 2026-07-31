// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/latex
//
// An optional (`latex-parser` feature) pest-based parser converting a useful
// subset of LaTeX math syntax into a `MathExpr` AST: `\frac{}{}`, postfix
// `^`/`_` scripts (which combine into a single `SubSup` when both are
// present), `\sqrt`/`\sqrt[n]`, `\left`/`\right` delimiter pairs, `{}`
// grouping, bare identifiers/numbers, spacing macros (`\,`, `\;`, `\!`,
// `\quad`, `\qquad`), and a curated table of common Greek-letter/operator/
// relation/binary-operator macros. This is a convenience layer on top of the
// AST, not a LaTeX engine: unsupported commands are a parse error, not a
// silent no-op or fallback.

use crate::ast::{FractionBar, Limits, MathExpr, MathSpace};
use crate::atom::AtomClass;
use crate::error::{MathError, Result};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "latex.pest"]
struct LatexGrammar;

/// Parse a LaTeX math string (e.g. `"\frac{x}{y^2}"`) into a [`MathExpr`].
pub fn parse(input: &str) -> Result<MathExpr> {
    let mut pairs =
        LatexGrammar::parse(Rule::program, input).map_err(|e| MathError::LatexParse(e.to_string()))?;
    let program = pairs.next().ok_or_else(|| MathError::LatexParse("empty input".to_string()))?;
    let expr_pair = program
        .into_inner()
        .find(|p| p.as_rule() == Rule::expr)
        .ok_or_else(|| MathError::LatexParse("empty expression".to_string()))?;
    build_expr(expr_pair)
}

fn build_expr(pair: Pair<Rule>) -> Result<MathExpr> {
    let mut items = Vec::new();
    for term_pair in pair.into_inner() {
        items.push(build_term(term_pair)?);
    }
    Ok(match items.len() {
        1 => items.into_iter().next().unwrap(),
        _ => MathExpr::Row(items),
    })
}

fn build_term(pair: Pair<Rule>) -> Result<MathExpr> {
    let mut inner = pair.into_inner();
    let base_pair = inner.next().ok_or_else(|| MathError::LatexParse("empty term".to_string()))?;
    let mut base = build_primary(base_pair)?;

    let mut pending_sub: Option<MathExpr> = None;
    let mut pending_sup: Option<MathExpr> = None;
    for script_pair in inner {
        let rule = script_pair.as_rule();
        let arg_pair = script_pair
            .into_inner()
            .next()
            .ok_or_else(|| MathError::LatexParse("empty sub/superscript".to_string()))?;
        let arg = build_primary(arg_pair)?;
        match rule {
            Rule::sup if pending_sup.is_none() => pending_sup = Some(arg),
            Rule::sub if pending_sub.is_none() => pending_sub = Some(arg),
            Rule::sup => {
                base = combine(base, pending_sub.take(), pending_sup.take());
                pending_sup = Some(arg);
            }
            Rule::sub => {
                base = combine(base, pending_sub.take(), pending_sup.take());
                pending_sub = Some(arg);
            }
            other => return Err(MathError::LatexParse(alloc::format!("unexpected script rule: {other:?}"))),
        }
    }
    Ok(combine(base, pending_sub, pending_sup))
}

fn combine(base: MathExpr, sub: Option<MathExpr>, sup: Option<MathExpr>) -> MathExpr {
    match (sub, sup) {
        (None, None) => base,
        (Some(sub), None) => MathExpr::Subscript { base: Box::new(base), sub: Box::new(sub) },
        (None, Some(sup)) => MathExpr::Superscript { base: Box::new(base), sup: Box::new(sup) },
        (Some(sub), Some(sup)) => {
            MathExpr::SubSup { base: Box::new(base), sub: Box::new(sub), sup: Box::new(sup) }
        }
    }
}

fn build_primary(pair: Pair<Rule>) -> Result<MathExpr> {
    match pair.as_rule() {
        Rule::group => {
            let inner = pair
                .into_inner()
                .next()
                .ok_or_else(|| MathError::LatexParse("empty group".to_string()))?;
            build_expr(inner)
        }
        Rule::frac => build_frac(pair),
        Rule::sqrt => build_sqrt(pair),
        Rule::left_right => build_left_right(pair),
        Rule::punct_space => Ok(MathExpr::Spacing(match pair.as_str() {
            "\\," => MathSpace::Thin,
            "\\;" => MathSpace::Thick,
            "\\!" => MathSpace::NegativeThin,
            other => return Err(MathError::LatexParse(alloc::format!("unknown spacing command: {other}"))),
        })),
        Rule::command => build_command(pair),
        Rule::number => Ok(MathExpr::Number(pair.as_str().to_string())),
        Rule::identifier => Ok(MathExpr::Identifier(pair.as_str().to_string())),
        Rule::symbol => Ok(build_symbol(pair.as_str())),
        other => Err(MathError::LatexParse(alloc::format!("unexpected token: {other:?}"))),
    }
}

fn build_frac(pair: Pair<Rule>) -> Result<MathExpr> {
    let mut inner = pair.into_inner();
    let num = inner.next().ok_or_else(|| MathError::LatexParse("\\frac missing numerator".to_string()))?;
    let den = inner
        .next()
        .ok_or_else(|| MathError::LatexParse("\\frac missing denominator".to_string()))?;
    Ok(MathExpr::Fraction {
        numerator: Box::new(build_primary(num)?),
        denominator: Box::new(build_primary(den)?),
        bar: FractionBar::Default,
    })
}

fn build_sqrt(pair: Pair<Rule>) -> Result<MathExpr> {
    let mut inner: Vec<Pair<Rule>> = pair.into_inner().collect();
    let radicand_pair = inner
        .pop()
        .ok_or_else(|| MathError::LatexParse("\\sqrt missing radicand".to_string()))?;
    let index = match inner.pop() {
        Some(idx_pair) => Some(Box::new(build_expr(idx_pair)?)),
        None => None,
    };
    Ok(MathExpr::Radical { index, radicand: Box::new(build_primary(radicand_pair)?) })
}

fn build_left_right(pair: Pair<Rule>) -> Result<MathExpr> {
    let mut inner = pair.into_inner();
    let left = inner
        .next()
        .ok_or_else(|| MathError::LatexParse("\\left missing delimiter".to_string()))?;
    let body = inner.next().ok_or_else(|| MathError::LatexParse("\\left missing body".to_string()))?;
    let right = inner
        .next()
        .ok_or_else(|| MathError::LatexParse("\\right missing delimiter".to_string()))?;
    Ok(MathExpr::DelimiterPair {
        left: parse_delim(left.as_str()),
        body: Box::new(build_expr(body)?),
        right: parse_delim(right.as_str()),
    })
}

fn parse_delim(s: &str) -> Option<char> {
    match s {
        "." => None,
        _ => s.chars().next(),
    }
}

fn build_command(pair: Pair<Rule>) -> Result<MathExpr> {
    let name = &pair.as_str()[1..]; // strip the leading backslash
    match name {
        "quad" => return Ok(MathExpr::Spacing(MathSpace::Quad)),
        "qquad" => return Ok(MathExpr::Spacing(MathSpace::Custom(2.0))),
        _ => {}
    }
    if let Some((ch, class)) = macro_symbol(name) {
        return Ok(MathExpr::Symbol { ch, class });
    }
    if let Some(expr) = macro_operator(name) {
        return Ok(expr);
    }
    Err(MathError::LatexParse(alloc::format!("unknown command: \\{name}")))
}

fn build_symbol(s: &str) -> MathExpr {
    let ch = s.chars().next().unwrap_or(' ');
    let class = match ch {
        '+' | '-' | '*' | '/' => AtomClass::Bin,
        '=' | '<' | '>' => AtomClass::Rel,
        ',' | ';' => AtomClass::Punct,
        '(' | '[' => AtomClass::Open,
        ')' | ']' => AtomClass::Close,
        _ => AtomClass::Ord,
    };
    MathExpr::Symbol { ch, class }
}

fn macro_operator(name: &str) -> Option<MathExpr> {
    let (ch, limits) = match name {
        "sum" => (Some('\u{2211}'), Limits::Auto),
        "prod" => (Some('\u{220F}'), Limits::Auto),
        "int" => (Some('\u{222B}'), Limits::NoLimits),
        "oint" => (Some('\u{222E}'), Limits::NoLimits),
        "lim" => (None, Limits::Limits),
        "max" => (None, Limits::Limits),
        "min" => (None, Limits::Limits),
        _ => return None,
    };
    Some(MathExpr::Operator { name: name.to_string(), ch, limits })
}

/// Common macro names mapped to a Unicode symbol and its TeX atom class.
/// Deliberately a curated subset (Greek letters, common relations/binary
/// operators/misc symbols), not an exhaustive LaTeX symbol table.
fn macro_symbol(name: &str) -> Option<(char, AtomClass)> {
    use AtomClass::{Bin, Ord, Rel};
    Some(match name {
        "alpha" => ('\u{3B1}', Ord),
        "beta" => ('\u{3B2}', Ord),
        "gamma" => ('\u{3B3}', Ord),
        "delta" => ('\u{3B4}', Ord),
        "epsilon" => ('\u{3B5}', Ord),
        "zeta" => ('\u{3B6}', Ord),
        "eta" => ('\u{3B7}', Ord),
        "theta" => ('\u{3B8}', Ord),
        "iota" => ('\u{3B9}', Ord),
        "kappa" => ('\u{3BA}', Ord),
        "lambda" => ('\u{3BB}', Ord),
        "mu" => ('\u{3BC}', Ord),
        "nu" => ('\u{3BD}', Ord),
        "xi" => ('\u{3BE}', Ord),
        "omicron" => ('\u{3BF}', Ord),
        "pi" => ('\u{3C0}', Ord),
        "rho" => ('\u{3C1}', Ord),
        "sigma" => ('\u{3C3}', Ord),
        "tau" => ('\u{3C4}', Ord),
        "upsilon" => ('\u{3C5}', Ord),
        "phi" => ('\u{3C6}', Ord),
        "chi" => ('\u{3C7}', Ord),
        "psi" => ('\u{3C8}', Ord),
        "omega" => ('\u{3C9}', Ord),
        "Gamma" => ('\u{393}', Ord),
        "Delta" => ('\u{394}', Ord),
        "Theta" => ('\u{398}', Ord),
        "Lambda" => ('\u{39B}', Ord),
        "Xi" => ('\u{39E}', Ord),
        "Pi" => ('\u{3A0}', Ord),
        "Sigma" => ('\u{3A3}', Ord),
        "Upsilon" => ('\u{3A5}', Ord),
        "Phi" => ('\u{3A6}', Ord),
        "Psi" => ('\u{3A8}', Ord),
        "Omega" => ('\u{3A9}', Ord),
        "leq" | "le" => ('\u{2264}', Rel),
        "geq" | "ge" => ('\u{2265}', Rel),
        "neq" | "ne" => ('\u{2260}', Rel),
        "approx" => ('\u{2248}', Rel),
        "equiv" => ('\u{2261}', Rel),
        "sim" => ('\u{223C}', Rel),
        "subset" => ('\u{2282}', Rel),
        "supset" => ('\u{2283}', Rel),
        "in" => ('\u{2208}', Rel),
        "to" | "rightarrow" => ('\u{2192}', Rel),
        "pm" => ('\u{B1}', Bin),
        "mp" => ('\u{2213}', Bin),
        "times" => ('\u{D7}', Bin),
        "div" => ('\u{F7}', Bin),
        "cdot" => ('\u{22C5}', Bin),
        "infty" => ('\u{221E}', Ord),
        "partial" => ('\u{2202}', Ord),
        "nabla" => ('\u{2207}', Ord),
        "forall" => ('\u{2200}', Ord),
        "exists" => ('\u{2203}', Ord),
        "emptyset" => ('\u{2205}', Ord),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_identifier() {
        assert_eq!(parse("x").unwrap(), MathExpr::Identifier("x".to_string()));
    }

    #[test]
    fn parses_number() {
        assert_eq!(parse("42").unwrap(), MathExpr::Number("42".to_string()));
    }

    #[test]
    fn parses_spec2_example_frac() {
        let expr = parse("\\frac{x}{y^2}").unwrap();
        let expected = MathExpr::Fraction {
            numerator: Box::new(MathExpr::Identifier("x".to_string())),
            denominator: Box::new(MathExpr::Superscript {
                base: Box::new(MathExpr::Identifier("y".to_string())),
                sup: Box::new(MathExpr::Number("2".to_string())),
            }),
            bar: FractionBar::Default,
        };
        assert_eq!(expr, expected);
    }

    #[test]
    fn combines_sub_and_sup_into_subsup() {
        let expr = parse("x_1^2").unwrap();
        match expr {
            MathExpr::SubSup { base, sub, sup } => {
                assert_eq!(*base, MathExpr::Identifier("x".to_string()));
                assert_eq!(*sub, MathExpr::Number("1".to_string()));
                assert_eq!(*sup, MathExpr::Number("2".to_string()));
            }
            other => panic!("expected SubSup, got {other:?}"),
        }
    }

    #[test]
    fn parses_sqrt_with_index() {
        let expr = parse("\\sqrt[3]{x}").unwrap();
        match expr {
            MathExpr::Radical { index: Some(index), radicand } => {
                assert_eq!(*index, MathExpr::Number("3".to_string()));
                assert_eq!(*radicand, MathExpr::Identifier("x".to_string()));
            }
            other => panic!("expected Radical with index, got {other:?}"),
        }
    }

    #[test]
    fn parses_left_right_delimiters() {
        let expr = parse("\\left(x\\right)").unwrap();
        match expr {
            MathExpr::DelimiterPair { left: Some('('), right: Some(')'), .. } => {}
            other => panic!("expected DelimiterPair, got {other:?}"),
        }
    }

    #[test]
    fn parses_greek_letter_macro() {
        assert_eq!(parse("\\alpha").unwrap(), MathExpr::Symbol { ch: '\u{3B1}', class: AtomClass::Ord });
    }

    #[test]
    fn parses_sum_operator_with_limits() {
        let expr = parse("\\sum").unwrap();
        match expr {
            MathExpr::Operator { name, limits: Limits::Auto, .. } => assert_eq!(name, "sum"),
            other => panic!("expected Operator, got {other:?}"),
        }
    }

    #[test]
    fn unknown_command_is_an_error() {
        assert!(parse("\\notarealcommand").is_err());
    }

    #[test]
    fn row_of_multiple_terms() {
        let expr = parse("x+y").unwrap();
        match expr {
            MathExpr::Row(items) => assert_eq!(items.len(), 3),
            other => panic!("expected Row, got {other:?}"),
        }
    }
}
