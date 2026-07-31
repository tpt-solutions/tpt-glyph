// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/prelude
//
// Convenience re-exports for the crate's typical `use tpt_glyph_math::prelude::*;` usage.

pub use crate::ast::{DisplayStyleKind, FractionBar, Limits, MathExpr, MathSpace};
pub use crate::atom::AtomClass;
pub use crate::style::MathStyle;

#[cfg(feature = "std")]
pub use crate::emit::{typeset, typeset_to_render_tree, RenderTarget};

#[cfg(feature = "latex-parser")]
pub use crate::latex::parse as parse_latex;
