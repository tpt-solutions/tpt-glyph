// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math

//! TeX-style math typesetting: a strongly-typed [`ast::MathExpr`] AST, the
//! TeXbook Chapter 17 layout algorithm ([`layout`]), and (with the `std`
//! feature) emission to `tpt-glyph-core` draw commands ([`emit`]).
//!
//! `no_std` (+ `alloc`) by default: the AST and layout algorithm only need
//! `tpt-glyph-font` for glyph metrics. Enable `std` for emission (which
//! depends on `tpt-glyph-core`, not yet `no_std` itself) and `latex-parser`
//! for the optional LaTeX-string-to-[`ast::MathExpr`] parser ([`latex`]).
//!
//! ```
//! use tpt_glyph_math::prelude::*;
//!
//! // x / y^2 — the worked example from the project spec.
//! let expr = MathExpr::Fraction {
//!     numerator: Box::new(MathExpr::Identifier("x".into())),
//!     denominator: Box::new(MathExpr::Superscript {
//!         base: Box::new(MathExpr::Identifier("y".into())),
//!         sup: Box::new(MathExpr::Number("2".into())),
//!     }),
//!     bar: FractionBar::Default,
//! };
//! # let _ = expr;
//! ```
//!
//! With the `latex-parser` feature, the same tree can be built from a LaTeX
//! string instead:
//!
//! ```
//! # #[cfg(feature = "latex-parser")] {
//! use tpt_glyph_math::prelude::*;
//!
//! let expr = parse_latex(r"\frac{x}{y^2}").unwrap();
//! assert!(matches!(expr, MathExpr::Fraction { .. }));
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[doc = include_str!("../examples/quickstart.md")]
pub mod ast;
pub mod atom;
pub mod constants;
#[cfg(feature = "std")]
pub mod emit;
pub mod error;
#[cfg(feature = "latex-parser")]
pub mod latex;
pub mod layout;
pub mod prelude;
pub mod style;
