// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/emit
//
// Walks a laid-out `MathBox` tree (see `layout`) into `tpt-glyph-core` draw
// commands: glyph outlines become filled `Path`s (nonzero winding across all
// of a glyph's contours), rules become filled rectangles. This is the only
// module that depends on `tpt-glyph-core`, which is why it's gated behind
// this crate's `std` feature (`tpt-glyph-core` isn't `no_std` yet).

use crate::ast::MathExpr;
use crate::constants::MathConstants;
use crate::layout::{layout, BoxContent, MathBox};
use crate::style::MathStyle;
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath};
use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
use tpt_glyph_core::render::{DrawCommand, RenderTree};
use tpt_glyph_font::{Font, GlyphId, Segment};

/// Typeset `expr` at `style`/`font_size` using `font`, filled with `color`.
///
/// Returns the raw draw commands in "math user space" — origin at the
/// formula's own baseline, x rightward, y upward, matching the returned
/// [`MathBox`]'s own metrics — so a caller can position/merge them into a
/// larger page's own coordinate system. Use [`typeset_to_render_tree`] for a
/// standalone, ready-to-rasterize [`RenderTree`].
pub fn typeset(
    expr: &MathExpr,
    font: &Font,
    font_size: f64,
    style: MathStyle,
    color: RgbColor,
) -> (Vec<DrawCommand>, MathBox) {
    typeset_at(expr, font, font_size, style, color, Point::new(0.0, 0.0))
}

/// Where to place a typeset formula within a standalone [`RenderTree`]:
/// the canvas dimensions, and the point the formula's own baseline origin
/// should be placed at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderTarget {
    pub width: u32,
    pub height: u32,
    pub origin: Point,
}

/// Typeset `expr` directly into a standalone, ready-to-rasterize [`RenderTree`]
/// sized and positioned per `target`.
pub fn typeset_to_render_tree(
    expr: &MathExpr,
    font: &Font,
    font_size: f64,
    style: MathStyle,
    color: RgbColor,
    target: RenderTarget,
) -> RenderTree {
    let (commands, _) = typeset_at(expr, font, font_size, style, color, target.origin);
    RenderTree {
        width: target.width,
        height: target.height,
        commands,
    }
}

fn typeset_at(
    expr: &MathExpr,
    font: &Font,
    font_size: f64,
    style: MathStyle,
    color: RgbColor,
    origin: Point,
) -> (Vec<DrawCommand>, MathBox) {
    let k = MathConstants::from_font(font, font_size);
    let math_box = layout(expr, style, font, &k, font_size);
    let mut tree = RenderTree::new(0, 0);
    let state = GraphicsState::new().with_fill_color(color);
    emit_box(&math_box, origin, font, &state, &mut tree);
    (tree.commands, math_box)
}

fn emit_box(b: &MathBox, origin: Point, font: &Font, state: &GraphicsState, tree: &mut RenderTree) {
    match &b.content {
        BoxContent::Glyph {
            gid,
            font_scale,
            y_scale,
            y_shift,
        } => {
            let path = glyph_to_path(font, *gid, *font_scale, *y_scale, *y_shift, origin);
            if !path.is_empty() {
                tree.fill(state, path);
            }
        }
        BoxContent::Rule { thickness } => {
            tree.fill(state, rect_path(origin, b.width, *thickness));
        }
        BoxContent::HList(items) | BoxContent::VList(items) => {
            for item in items {
                let child_origin = Point::new(origin.x + item.dx, origin.y + item.dy);
                emit_box(&item.b, child_origin, font, state, tree);
            }
        }
        BoxContent::Empty => {}
    }
}

/// Convert one glyph's outline to a `Path`: every contour becomes a closed
/// subpath (nonzero winding fills correctly across multiple contours, e.g.
/// the hole in an "o"), quadratic segments are degree-elevated to cubic, and
/// coordinates go from font design units to absolute math-user-space points
/// via `font_scale` (em size), `y_scale`/`y_shift` (the stretchy-glyph
/// transform computed by `layout::scaled_glyph_box`), and `origin`.
fn glyph_to_path(
    font: &Font,
    gid: GlyphId,
    font_scale: f64,
    y_scale: f64,
    y_shift: f64,
    origin: Point,
) -> Path {
    let Some(outline) = font.glyph_outline(gid) else {
        return Path::new();
    };
    let units_per_em = (font.units_per_em().max(1)) as f64;
    let to_point = |p: tpt_glyph_font::Point| -> Point {
        let x = (p.x as f64 / units_per_em) * font_scale;
        let y = (p.y as f64 / units_per_em) * font_scale * y_scale + y_shift;
        Point::new(origin.x + x, origin.y + y)
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

fn rect_path(origin: Point, width: f64, height: f64) -> Path {
    let p0 = origin;
    let p1 = Point::new(origin.x + width, origin.y);
    let p2 = Point::new(origin.x + width, origin.y + height);
    let p3 = Point::new(origin.x, origin.y + height);
    let mut sp = Subpath::new(p0);
    sp.push_curve(line_as_cubic(p0, p1));
    sp.push_curve(line_as_cubic(p1, p2));
    sp.push_curve(line_as_cubic(p2, p3));
    sp.push_curve(line_as_cubic(p3, p0));
    sp.closed = true;
    Path { subpaths: vec![sp] }
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
    use crate::ast::FractionBar;
    use tpt_glyph_core::render::{DebugRasterizer, Rasterizer};

    fn sample_font() -> Font {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found");
        Font::from_bytes(&data).expect("valid font")
    }

    fn spec2_example() -> MathExpr {
        MathExpr::Fraction {
            numerator: Box::new(MathExpr::Identifier("x".to_string())),
            denominator: Box::new(MathExpr::Superscript {
                base: Box::new(MathExpr::Identifier("y".to_string())),
                sup: Box::new(MathExpr::Number("2".to_string())),
            }),
            bar: FractionBar::Default,
        }
    }

    #[test]
    fn typeset_produces_nonempty_fill_commands() {
        let font = sample_font();
        let (commands, math_box) = typeset(
            &spec2_example(),
            &font,
            40.0,
            MathStyle::Display,
            RgbColor::BLACK,
        );
        assert!(!commands.is_empty());
        assert!(commands
            .iter()
            .all(|c| matches!(c, DrawCommand::Fill { .. })));
        assert!(math_box.width > 0.0);
    }

    #[test]
    fn spec2_example_rasterizes_to_a_visibly_nonwhite_canvas() {
        let font = sample_font();
        let math_box = layout(
            &spec2_example(),
            MathStyle::Display,
            &font,
            &MathConstants::from_font(&font, 40.0),
            40.0,
        );

        let margin = 20.0;
        let width = (math_box.width + 2.0 * margin).ceil().max(1.0) as u32;
        let height = (math_box.height + math_box.depth + 2.0 * margin)
            .ceil()
            .max(1.0) as u32;
        let origin = Point::new(margin, margin + math_box.depth);

        let tree = typeset_to_render_tree(
            &spec2_example(),
            &font,
            40.0,
            MathStyle::Display,
            RgbColor::BLACK,
            RenderTarget {
                width,
                height,
                origin,
            },
        );
        let canvas = DebugRasterizer.rasterize(&tree).unwrap();
        assert!(canvas
            .pixels
            .iter()
            .any(|p| p.r == 0 && p.g == 0 && p.b == 0));
    }
}
