// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math
//
// TeX-style math typesetting: a strongly-typed `MathExpr` AST, the TeXbook
// Chapter 17 layout algorithm, and emission to `tpt-glyph-core` draw
// commands. `no_std` (+ `alloc`) by default; enable the `std` feature for
// emission (which depends on `tpt-glyph-core`) and `latex-parser` for the
// optional LaTeX-string-to-`MathExpr` parser.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

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
