// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-typeset / emit
//
// Turns a `LaidOutPage` into a `tpt-glyph-core` `RenderTree`: word glyphs
// become filled vector outlines (the same technique as
// `tpt-glyph-math::emit`, reimplemented locally here since that routine
// isn't part of `tpt-glyph-math`'s public API), and inline math is emitted
// via `tpt-glyph-math::emit::typeset` and translated into place.

use crate::layout::{LaidOutPage, PageGeometry, PlacedItem};
use crate::Block;
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath, Transform};
use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
use tpt_glyph_core::render::{DrawCommand, RenderTree};
use tpt_glyph_font::{Font, GlyphId, GlyphOutline, Segment};
use tpt_glyph_math::style::MathStyle;

/// Lay out `blocks` and emit one [`RenderTree`] per page, sized per `page`,
/// with all glyphs filled in black.
pub fn typeset_to_render_trees(blocks: &[Block], page: PageGeometry) -> Vec<RenderTree> {
    crate::layout::typeset(blocks, page)
        .iter()
        .map(|laid_out| page_to_render_tree(laid_out, page, RgbColor::BLACK))
        .collect()
}

/// Emit a single laid-out page into a [`RenderTree`] sized per `page`.
///
/// Layout coordinates are page user-space (origin bottom-left, y increasing
/// upward); the canvas is row-major with the top-left pixel at index 0, so
/// every command's CTM carries the same page-to-canvas flip used by the
/// PDF/PostScript rendering path (`GraphicsState::with_page_flip`).
pub fn page_to_render_tree(
    laid_out: &LaidOutPage,
    page: PageGeometry,
    color: RgbColor,
) -> RenderTree {
    let mut tree = RenderTree::new(
        page.width.ceil().max(1.0) as u32,
        page.height.ceil().max(1.0) as u32,
    );
    let state = GraphicsState::new()
        .with_fill_color(color)
        .with_page_flip(page.height);
    let flip = state.ctm;

    for item in &laid_out.items {
        match item {
            PlacedItem::Word {
                font,
                size,
                x,
                y,
                text,
            } => {
                emit_word(&mut tree, &state, font, text, *size, *x, *y);
            }
            PlacedItem::Math {
                font,
                size,
                x,
                y,
                expr,
            } => {
                let (commands, _) =
                    tpt_glyph_math::emit::typeset(expr, font, *size, MathStyle::Text, color);
                // Apply the translation into page position first, then the
                // page-to-canvas flip (matches `GraphicsState::concat_transform`'s
                // "innermost transform first" convention).
                let combined = Transform::new(1.0, 0.0, 0.0, 1.0, *x, *y).concat(&flip);
                tree.commands
                    .extend(commands.into_iter().map(|c| translate(c, &combined)));
            }
        }
    }

    tree
}

fn translate(cmd: DrawCommand, t: &Transform) -> DrawCommand {
    match cmd {
        DrawCommand::Fill { path, color, ctm } => DrawCommand::Fill {
            path,
            color,
            ctm: t.concat(&ctm),
        },
        DrawCommand::FillEvenOdd { path, color, ctm } => DrawCommand::FillEvenOdd {
            path,
            color,
            ctm: t.concat(&ctm),
        },
        DrawCommand::Stroke {
            path,
            color,
            line_width,
            ctm,
        } => DrawCommand::Stroke {
            path,
            color,
            line_width,
            ctm: t.concat(&ctm),
        },
        DrawCommand::Clip { path, ctm } => DrawCommand::Clip {
            path,
            ctm: t.concat(&ctm),
        },
    }
}

fn emit_word(
    tree: &mut RenderTree,
    state: &GraphicsState,
    font: &Font,
    text: &str,
    size: f64,
    origin_x: f64,
    y: f64,
) {
    let upm = font.units_per_em().max(1) as f64;
    let mut x = origin_x;
    for c in text.chars() {
        let Some(gid) = font.glyph_for_char(c) else {
            continue;
        };
        if let Some(outline) = font.glyph_outline(gid) {
            let path = glyph_to_path(&outline, upm, size, x, y);
            if !path.is_empty() {
                tree.fill(state, path);
            }
        }
        x += advance(font, gid, upm, size);
    }
}

fn advance(font: &Font, gid: GlyphId, upm: f64, size: f64) -> f64 {
    font.glyph_advance(gid)
        .map(|a| a as f64 / upm * size)
        .unwrap_or(0.0)
}

/// Convert one glyph's outline (font design units) into a `Path` in
/// absolute page-space points: every contour becomes a closed subpath
/// (quadratic segments degree-elevated to cubic), scaled by `size`/`upm`
/// and offset by `(origin_x, y)`.
fn glyph_to_path(outline: &GlyphOutline, upm: f64, size: f64, origin_x: f64, y: f64) -> Path {
    let to_point = |p: tpt_glyph_font::Point| -> Point {
        Point::new(
            origin_x + (p.x as f64 / upm) * size,
            y + (p.y as f64 / upm) * size,
        )
    };

    let mut path = Path::new();
    for contour in &outline.contours {
        let mut cur = contour.start;
        let mut sp = Subpath::new(to_point(cur));
        for seg in &contour.segments {
            match seg {
                Segment::LineTo(p) => {
                    let start = to_point(cur);
                    let end = to_point(*p);
                    sp.push_curve(line_as_cubic(start, end));
                    cur = *p;
                }
                Segment::QuadTo { control, to } => {
                    let start = to_point(cur);
                    let c = to_point(*control);
                    let end = to_point(*to);
                    sp.push_curve(CubicBezier {
                        start,
                        control1: lerp(start, c, 2.0 / 3.0),
                        control2: lerp(end, c, 2.0 / 3.0),
                        end,
                    });
                    cur = *to;
                }
                Segment::CurveTo {
                    control1,
                    control2,
                    to,
                } => {
                    let start = to_point(cur);
                    sp.push_curve(CubicBezier {
                        start,
                        control1: to_point(*control1),
                        control2: to_point(*control2),
                        end: to_point(*to),
                    });
                    cur = *to;
                }
            }
        }
        sp.closed = true;
        path.subpaths.push(sp);
    }
    path
}

fn line_as_cubic(start: Point, end: Point) -> CubicBezier {
    CubicBezier {
        start,
        control1: lerp(start, end, 1.0 / 3.0),
        control2: lerp(start, end, 2.0 / 3.0),
        end,
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Paragraph, ParagraphItem};
    use std::sync::Arc;
    use tpt_glyph_core::render::{DebugRasterizer, Rasterizer};

    fn sample_font() -> Arc<Font> {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found");
        Arc::new(Font::from_bytes(&data).expect("valid font"))
    }

    #[test]
    fn word_emits_nonempty_fill_commands() {
        let font = sample_font();
        let paragraph = Paragraph::new(font, 24.0, vec![ParagraphItem::Text("Hi".to_string())]);
        let page = PageGeometry {
            width: 200.0,
            height: 200.0,
            margin: 20.0,
        };
        let trees = typeset_to_render_trees(&[Block::Paragraph(paragraph)], page);
        assert_eq!(trees.len(), 1);
        assert!(!trees[0].commands.is_empty());
        assert!(trees[0]
            .commands
            .iter()
            .all(|c| matches!(c, DrawCommand::Fill { .. })));
    }

    #[test]
    fn rendered_page_is_visibly_nonwhite() {
        let font = sample_font();
        let paragraph = Paragraph::new(font, 24.0, vec![ParagraphItem::Text("Hello".to_string())]);
        let page = PageGeometry {
            width: 200.0,
            height: 200.0,
            margin: 20.0,
        };
        let trees = typeset_to_render_trees(&[Block::Paragraph(paragraph)], page);
        let canvas = DebugRasterizer.rasterize(&trees[0]).unwrap();
        assert!(canvas
            .pixels
            .iter()
            .any(|p| p.r == 0 && p.g == 0 && p.b == 0));
    }

    #[test]
    fn inline_math_produces_translated_commands() {
        use tpt_glyph_math::ast::MathExpr;
        let font = sample_font();
        let paragraph = Paragraph::new(
            font,
            20.0,
            vec![
                ParagraphItem::Text("x =".to_string()),
                ParagraphItem::Math(MathExpr::Identifier("y".to_string())),
            ],
        );
        let page = PageGeometry {
            width: 200.0,
            height: 200.0,
            margin: 20.0,
        };
        let trees = typeset_to_render_trees(&[Block::Paragraph(paragraph)], page);
        assert!(!trees[0].commands.is_empty());
    }
}
