// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/ast
//
// The strongly-typed `MathExpr` AST. Building an expression by hand (or via
// the optional `latex-parser` feature) and handing it to `layout`/`emit` is
// the entire public surface of this crate — no runtime macro-expansion.

use crate::atom::AtomClass;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// A math expression node.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MathExpr {
    /// A variable, rendered in math-italic, e.g. `x`.
    Identifier(String),
    /// A literal number, rendered upright, e.g. `12`.
    Number(String),
    /// A single symbol with an explicit atom classification, e.g. `+` (Bin)
    /// or `=` (Rel).
    Symbol { ch: char, class: AtomClass },
    /// A (possibly large) operator, e.g. `\sum`, `\int`, `\lim`.
    Operator {
        name: String,
        ch: Option<char>,
        limits: Limits,
    },
    /// A sequence of expressions laid out left-to-right as one row, with
    /// interelement spacing applied between them.
    Row(Vec<MathExpr>),
    /// A generalized fraction: `numerator` over `denominator`, separated by
    /// a rule per `bar`.
    Fraction {
        numerator: Box<MathExpr>,
        denominator: Box<MathExpr>,
        bar: FractionBar,
    },
    Superscript {
        base: Box<MathExpr>,
        sup: Box<MathExpr>,
    },
    Subscript {
        base: Box<MathExpr>,
        sub: Box<MathExpr>,
    },
    SubSup {
        base: Box<MathExpr>,
        sub: Box<MathExpr>,
        sup: Box<MathExpr>,
    },
    /// A radical: `\sqrt{radicand}` (`index: None`) or `\sqrt[index]{radicand}`.
    Radical {
        index: Option<Box<MathExpr>>,
        radicand: Box<MathExpr>,
    },
    /// A horizontal rule drawn above `body` (`\overline`).
    OverLine(Box<MathExpr>),
    /// A horizontal rule drawn below `body` (`\underline`).
    UnderLine(Box<MathExpr>),
    /// An accent mark (e.g. `^`, `~`, `.`) centered above `base`.
    Accent { base: Box<MathExpr>, accent: char },
    /// `\left <left> body \right <right>`: a sub-formula flanked by
    /// delimiters sized to its height/depth. Either delimiter may be absent
    /// (LaTeX's `\left.`/`\right.`).
    DelimiterPair {
        left: Option<char>,
        body: Box<MathExpr>,
        right: Option<char>,
    },
    /// An explicit style change (`\displaystyle`, `\textstyle`, ...).
    StyleOverride {
        style: DisplayStyleKind,
        body: Box<MathExpr>,
    },
    /// An explicit horizontal skip (`\,`, `\quad`, ...) with no visible glyph.
    Spacing(MathSpace),
}

/// Whether a big operator's limits are drawn above/below (Display style) or
/// as a regular sub/superscript, and whether that choice is forced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Limits {
    /// Display style: limits above/below. Otherwise: sub/superscript.
    Auto,
    /// Always draw limits above/below, regardless of style.
    Limits,
    /// Always draw as a regular sub/superscript, regardless of style.
    NoLimits,
}

/// The rule drawn between a fraction's numerator and denominator.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FractionBar {
    /// The font's default fraction rule thickness.
    Default,
    /// An explicit rule thickness, in the same units as the font size.
    Thickness(f64),
    /// No rule at all (LaTeX's `\atop`).
    None,
}

/// The four base math styles, as named in `\displaystyle`/`\textstyle`/
/// `\scriptstyle`/`\scriptscriptstyle` — never cramped; crampedness is
/// derived contextually by the layout algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DisplayStyleKind {
    Display,
    Text,
    Script,
    ScriptScript,
}

/// An explicit horizontal space with no glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MathSpace {
    /// `\,` — 3/18 em.
    Thin,
    /// `\:`/`\>` — 4/18 em.
    Medium,
    /// `\;` — 5/18 em.
    Thick,
    /// `\quad` — 1 em.
    Quad,
    /// `\!` — -3/18 em.
    NegativeThin,
    /// An explicit width, in em.
    Custom(f64),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape from spec2.txt's worked example: `x / y^2`.
    #[test]
    fn builds_fraction_of_superscript() {
        let expr = MathExpr::Fraction {
            numerator: Box::new(MathExpr::Identifier("x".into())),
            denominator: Box::new(MathExpr::Superscript {
                base: Box::new(MathExpr::Identifier("y".into())),
                sup: Box::new(MathExpr::Number("2".into())),
            }),
            bar: FractionBar::Default,
        };
        match expr {
            MathExpr::Fraction {
                numerator,
                denominator,
                bar,
            } => {
                assert_eq!(*numerator, MathExpr::Identifier("x".into()));
                assert_eq!(bar, FractionBar::Default);
                assert!(matches!(*denominator, MathExpr::Superscript { .. }));
            }
            _ => panic!("wrong variant"),
        }
    }
}
