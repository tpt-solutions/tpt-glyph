// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-typeset / layout
//
// Paragraph line-breaking, justification, and page pagination. Pure
// geometry: this module only needs glyph advance widths and inline math
// box metrics, not a rasterizer — `emit` handles turning the result into
// draw commands.

use crate::{Alignment, Block, Paragraph, ParagraphItem};
use std::sync::Arc;
use tpt_glyph_font::Font;
use tpt_glyph_math::ast::MathExpr;
use tpt_glyph_math::constants::MathConstants;
use tpt_glyph_math::layout::layout as layout_math;
use tpt_glyph_math::style::MathStyle;

/// A page's usable area: the full media size plus a uniform margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    pub width: f64,
    pub height: f64,
    pub margin: f64,
}

impl PageGeometry {
    pub fn content_width(&self) -> f64 {
        (self.width - 2.0 * self.margin).max(0.0)
    }
}

/// One item placed at an absolute position on a page, in page user-space
/// units (origin bottom-left, y increasing upward — PDF/PostScript
/// convention, matching `tpt-glyph-core`).
#[derive(Clone)]
pub enum PlacedItem {
    /// A run of text with no internal breakpoints, set in `font` at `size`,
    /// with its baseline at `(x, y)`.
    Word {
        font: Arc<Font>,
        size: f64,
        x: f64,
        y: f64,
        text: String,
    },
    /// An inline math formula, whose own baseline origin is at `(x, y)`.
    Math {
        font: Arc<Font>,
        size: f64,
        x: f64,
        y: f64,
        expr: MathExpr,
    },
}

/// One page's worth of placed items.
#[derive(Clone, Default)]
pub struct LaidOutPage {
    pub items: Vec<PlacedItem>,
}

/// Lay out `blocks` into pages sized per `page`.
pub fn typeset(blocks: &[Block], page: PageGeometry) -> Vec<LaidOutPage> {
    let mut pages = vec![LaidOutPage::default()];
    let top = page.height - page.margin;
    let bottom = page.margin;
    let mut cursor_y = top;

    for block in blocks {
        match block {
            Block::PageBreak => {
                pages.push(LaidOutPage::default());
                cursor_y = top;
            }
            Block::Paragraph(p) => {
                let lines = break_paragraph(p, page.content_width());
                for line in &lines {
                    let mut baseline = cursor_y - line.ascent;
                    if baseline - line.descent < bottom && cursor_y < top {
                        // Doesn't fit on the current page (and the page
                        // isn't already empty) — start a fresh one.
                        pages.push(LaidOutPage::default());
                        cursor_y = top;
                        baseline = cursor_y - line.ascent;
                    }
                    place_line(
                        pages.last_mut().expect("at least one page"),
                        line,
                        page.margin,
                        baseline,
                        &p.font,
                        p.size,
                    );
                    cursor_y = baseline - line.descent;
                }
                cursor_y -= p.space_after;
            }
        }
    }

    pages
}

/// A single laid-out line: its tokens with local x-offsets from the line's
/// own left margin, plus the vertical metrics needed to place its baseline
/// under the previous line.
struct Line {
    tokens: Vec<(Token, f64)>, // (token, x offset from line start)
    ascent: f64,
    descent: f64,
}

#[derive(Clone)]
enum Token {
    Word(String),
    Math(MathExpr),
}

fn place_line(
    page: &mut LaidOutPage,
    line: &Line,
    margin: f64,
    baseline: f64,
    font: &Arc<Font>,
    size: f64,
) {
    for (token, x) in &line.tokens {
        let item = match token {
            Token::Word(text) => PlacedItem::Word {
                font: font.clone(),
                size,
                x: margin + x,
                y: baseline,
                text: text.clone(),
            },
            Token::Math(expr) => PlacedItem::Math {
                font: font.clone(),
                size,
                x: margin + x,
                y: baseline,
                expr: expr.clone(),
            },
        };
        page.items.push(item);
    }
}

/// Break `paragraph`'s content into lines that fit `max_width`, using a
/// greedy (first-fit) algorithm: keep adding tokens to the current line
/// until the next one wouldn't fit, then start a new line. Justified
/// paragraphs distribute the leftover width across inter-word gaps on every
/// line but the last.
fn break_paragraph(paragraph: &Paragraph, max_width: f64) -> Vec<Line> {
    let tokens = tokenize(paragraph);
    let font = &paragraph.font;
    let size = paragraph.size;
    let space_width = text_width(font, " ", size).max(size * 0.2);
    let (base_ascent, base_descent) = font_ascent_descent(font, size);

    let mut lines: Vec<Vec<(Token, f64)>> = Vec::new(); // (token, width)
    let mut current: Vec<(Token, f64)> = Vec::new();
    let mut current_width = 0.0;

    for token in tokens {
        let w = token_width(font, size, &token);
        let candidate = if current.is_empty() {
            w
        } else {
            current_width + space_width + w
        };
        if !current.is_empty() && candidate > max_width {
            lines.push(std::mem::take(&mut current));
            current_width = 0.0;
        }
        if !current.is_empty() {
            current_width += space_width;
        }
        current_width += w;
        current.push((token, w));
    }
    if !current.is_empty() {
        lines.push(current);
    }

    let line_count = lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(i, tokens)| {
            let is_last = i + 1 == line_count;
            build_line(
                tokens,
                font,
                size,
                space_width,
                max_width,
                base_ascent,
                base_descent,
                paragraph.alignment,
                is_last,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_line(
    tokens: Vec<(Token, f64)>,
    font: &Arc<Font>,
    size: f64,
    space_width: f64,
    max_width: f64,
    base_ascent: f64,
    base_descent: f64,
    alignment: Alignment,
    is_last: bool,
) -> Line {
    let natural_width: f64 = tokens.iter().map(|(_, w)| *w).sum::<f64>()
        + space_width * (tokens.len().saturating_sub(1)) as f64;

    let gap = if alignment == Alignment::Justify && !is_last && tokens.len() > 1 {
        space_width + (max_width - natural_width) / (tokens.len() - 1) as f64
    } else {
        space_width
    };

    let mut ascent = base_ascent;
    let mut descent = base_descent;
    let mut placed = Vec::with_capacity(tokens.len());
    let mut x = 0.0;
    for (token, w) in tokens {
        if let Token::Math(expr) = &token {
            let k = MathConstants::from_font(font, size);
            let math_box = layout_math(expr, MathStyle::Text, font, &k, size);
            ascent = ascent.max(math_box.height);
            descent = descent.max(math_box.depth);
        }
        placed.push((token, x));
        x += w + gap;
    }

    Line {
        tokens: placed,
        ascent,
        descent,
    }
}

fn tokenize(paragraph: &Paragraph) -> Vec<Token> {
    let mut tokens = Vec::new();
    for item in &paragraph.items {
        match item {
            ParagraphItem::Text(text) => {
                tokens.extend(text.split_whitespace().map(|w| Token::Word(w.to_string())));
            }
            ParagraphItem::Math(expr) => tokens.push(Token::Math(expr.clone())),
        }
    }
    tokens
}

fn token_width(font: &Font, size: f64, token: &Token) -> f64 {
    match token {
        Token::Word(w) => text_width(font, w, size),
        Token::Math(expr) => {
            let k = MathConstants::from_font(font, size);
            layout_math(expr, MathStyle::Text, font, &k, size).width
        }
    }
}

fn text_width(font: &Font, text: &str, size: f64) -> f64 {
    let upm = font.units_per_em().max(1) as f64;
    text.chars()
        .filter_map(|c| font.glyph_for_char(c).and_then(|g| font.glyph_advance(g)))
        .map(|a| a as f64 / upm * size)
        .sum()
}

fn font_ascent_descent(font: &Font, size: f64) -> (f64, f64) {
    let upm = font.units_per_em().max(1) as f64;
    (
        font.ascender() as f64 / upm * size,
        -(font.descender() as f64) / upm * size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Paragraph;

    fn sample_font() -> Arc<Font> {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found");
        Arc::new(Font::from_bytes(&data).expect("valid font"))
    }

    #[test]
    fn short_paragraph_fits_on_one_line() {
        let font = sample_font();
        let p = Paragraph::new(font, 12.0, vec![ParagraphItem::Text("a b c".to_string())]);
        let lines = break_paragraph(&p, 1000.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].tokens.len(), 3);
    }

    #[test]
    fn narrow_width_forces_multiple_lines() {
        let font = sample_font();
        let p = Paragraph::new(
            font,
            12.0,
            vec![ParagraphItem::Text(
                "one two three four five six seven eight".to_string(),
            )],
        );
        let narrow = break_paragraph(&p, 60.0);
        let wide = break_paragraph(&p, 10000.0);
        assert_eq!(wide.len(), 1);
        assert!(
            narrow.len() > 1,
            "expected multiple lines, got {}",
            narrow.len()
        );
    }

    #[test]
    fn justified_non_last_line_fills_the_full_width() {
        let font = sample_font();
        let p = Paragraph::new(
            font,
            12.0,
            vec![ParagraphItem::Text(
                "one two three four five six seven eight nine ten eleven twelve".to_string(),
            )],
        )
        .justified();
        let max_width = 150.0;
        let lines = break_paragraph(&p, max_width);
        assert!(lines.len() > 1);
        // The last token on a justified (non-final) line should end exactly
        // at the line's right margin.
        let first = &lines[0];
        let (_, last_x) = first.tokens.last().unwrap();
        let last_width = token_width(&p.font, p.size, &first.tokens.last().unwrap().0);
        assert!(
            (last_x + last_width - max_width).abs() < 1e-6,
            "justified line should reach {max_width}, got {}",
            last_x + last_width
        );
    }

    #[test]
    fn typeset_paginates_when_content_overflows() {
        let font = sample_font();
        let mut items = Vec::new();
        for i in 0..200 {
            items.push(ParagraphItem::Text(format!("line{i}")));
        }
        let paragraphs: Vec<Block> = items
            .into_iter()
            .map(|item| Block::Paragraph(Paragraph::new(font.clone(), 12.0, vec![item])))
            .collect();
        let page = PageGeometry {
            width: 200.0,
            height: 200.0,
            margin: 20.0,
        };
        let pages = typeset(&paragraphs, page);
        assert!(pages.len() > 1, "expected pagination across multiple pages");
        for p in &pages {
            assert!(!p.items.is_empty());
        }
    }

    #[test]
    fn explicit_page_break_starts_a_new_page() {
        let font = sample_font();
        let blocks = vec![
            Block::Paragraph(Paragraph::new(
                font.clone(),
                12.0,
                vec![ParagraphItem::Text("first".into())],
            )),
            Block::PageBreak,
            Block::Paragraph(Paragraph::new(
                font,
                12.0,
                vec![ParagraphItem::Text("second".into())],
            )),
        ];
        let page = PageGeometry {
            width: 200.0,
            height: 200.0,
            margin: 20.0,
        };
        let pages = typeset(&blocks, page);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].items.len(), 1);
        assert_eq!(pages[1].items.len(), 1);
    }

    #[test]
    fn inline_math_is_placed_as_its_own_token() {
        use tpt_glyph_math::ast::MathExpr;
        let font = sample_font();
        let p = Paragraph::new(
            font,
            12.0,
            vec![
                ParagraphItem::Text("solve".to_string()),
                ParagraphItem::Math(MathExpr::Identifier("x".to_string())),
                ParagraphItem::Text("for x".to_string()),
            ],
        );
        let lines = break_paragraph(&p, 1000.0);
        assert_eq!(lines.len(), 1);
        assert!(lines[0]
            .tokens
            .iter()
            .any(|(t, _)| matches!(t, Token::Math(_))));
    }
}
