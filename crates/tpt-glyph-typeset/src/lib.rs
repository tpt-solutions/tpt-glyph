// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-typeset
//
// High-level document typesetting on top of `tpt-glyph-font` (text metrics,
// glyph outlines) and `tpt-glyph-math` (inline formula layout): paragraph
// line-breaking (with optional justification), pagination with page
// breaks, and emission to `tpt-glyph-core` draw commands for rasterization.

//! # tpt-glyph-typeset
//!
//! ```
//! use std::sync::Arc;
//! use tpt_glyph_typeset::{typeset_to_render_trees, Block, PageGeometry, Paragraph, ParagraphItem};
//!
//! # fn font_bytes() -> Vec<u8> {
//! #     std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
//! #         .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
//! #         .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
//! #         .unwrap_or_default()
//! # }
//! # let bytes = font_bytes();
//! # if !bytes.is_empty() {
//! let font = Arc::new(tpt_glyph_font::Font::from_bytes(&bytes).unwrap());
//! let paragraph = Paragraph::new(
//!     font,
//!     12.0,
//!     vec![ParagraphItem::Text("Hello, typeset world.".to_string())],
//! )
//! .justified();
//!
//! let page = PageGeometry { width: 200.0, height: 200.0, margin: 20.0 };
//! let trees = typeset_to_render_trees(&[Block::Paragraph(paragraph)], page);
//! assert_eq!(trees.len(), 1);
//! # }
//! ```

pub mod emit;
pub mod layout;

use std::sync::Arc;
use tpt_glyph_font::Font;
use tpt_glyph_math::ast::MathExpr;

pub use emit::typeset_to_render_trees;
pub use layout::{typeset, LaidOutPage, PageGeometry, PlacedItem};

/// Paragraph text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    /// Distribute leftover width across inter-word gaps on every line but
    /// the paragraph's last.
    Justify,
}

/// One piece of a paragraph's content: a run of prose text (tokenized into
/// words at whitespace) or an inline math formula (kept as a single
/// unbreakable token).
#[derive(Clone)]
pub enum ParagraphItem {
    Text(String),
    Math(MathExpr),
}

/// A paragraph: uniformly-styled content laid out as wrapped lines.
#[derive(Clone)]
pub struct Paragraph {
    pub font: Arc<Font>,
    pub size: f64,
    pub items: Vec<ParagraphItem>,
    pub alignment: Alignment,
    /// Extra vertical space after the paragraph's last line, before the
    /// next block starts.
    pub space_after: f64,
}

impl Paragraph {
    /// A left-aligned paragraph with a default `space_after` of half the
    /// point size.
    pub fn new(font: Arc<Font>, size: f64, items: Vec<ParagraphItem>) -> Self {
        Self {
            font,
            size,
            items,
            alignment: Alignment::Left,
            space_after: size * 0.5,
        }
    }

    pub fn justified(mut self) -> Self {
        self.alignment = Alignment::Justify;
        self
    }

    pub fn with_space_after(mut self, space_after: f64) -> Self {
        self.space_after = space_after;
        self
    }
}

/// One unit of document flow: a paragraph, or a forced page break.
#[derive(Clone)]
pub enum Block {
    Paragraph(Paragraph),
    PageBreak,
}
