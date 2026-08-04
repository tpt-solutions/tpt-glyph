// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-cli / mathdemo
//
// Phase 13 CLI demo: typeset a LaTeX math string into a standalone one-page
// PDF. Math glyphs and fraction bars are emitted as filled vector paths (see
// `tpt-glyph-math::emit`), so the resulting PDF needs no embedded font
// resource — every `DrawCommand::Fill` becomes a PDF path-fill directly.

use tpt_glyph_core::geometry::{Path, Point, Transform};
use tpt_glyph_core::graphics_state::RgbColor;
use tpt_glyph_core::render::{DrawCommand, RenderTree};
use tpt_glyph_font::Font;
use tpt_glyph_math::ast::MathExpr;
use tpt_glyph_math::constants::MathConstants;
use tpt_glyph_math::emit::{typeset_to_render_tree, RenderTarget};
use tpt_glyph_math::layout::layout;
use tpt_glyph_math::style::MathStyle;
use tpt_glyph_pdf_writer::{Stream, Value, Writer};

/// Typeset `expr` at `font_size` using `font`, laid out with a margin around
/// the formula's own ink box, and return a complete one-page PDF's bytes.
pub fn render_math_to_pdf(expr: &MathExpr, font: &Font, font_size: f64) -> Vec<u8> {
    const MARGIN: f64 = 20.0;

    let constants = MathConstants::from_font(font, font_size);
    let math_box = layout(expr, MathStyle::Display, font, &constants, font_size);

    let width = (math_box.width + 2.0 * MARGIN).ceil().max(1.0) as u32;
    let height = (math_box.height + math_box.depth + 2.0 * MARGIN)
        .ceil()
        .max(1.0) as u32;
    let origin = Point::new(MARGIN, MARGIN + math_box.depth);

    let tree = typeset_to_render_tree(
        expr,
        font,
        font_size,
        MathStyle::Display,
        RgbColor::BLACK,
        RenderTarget {
            width,
            height,
            origin,
        },
    );

    build_pdf(&tree)
}

/// Wrap a [`RenderTree`]'s draw commands into a single-page PDF via
/// `tpt-glyph-pdf-writer`.
fn build_pdf(tree: &RenderTree) -> Vec<u8> {
    let mut content = Stream::new(content_stream(tree));
    content.compress();

    let mut w = Writer::new();
    let content_id = w.add_stream(content);
    let pages_id = w.alloc();
    let page_id = w.add(Value::dict([
        ("Type", Value::name("Page")),
        ("Parent", Value::reference(pages_id)),
        (
            "MediaBox",
            Value::array([
                Value::Integer(0),
                Value::Integer(0),
                Value::Real(tree.width as f64),
                Value::Real(tree.height as f64),
            ]),
        ),
        ("Resources", Value::Dict(Vec::new())),
        ("Contents", Value::reference(content_id)),
    ]));
    w.define(
        pages_id,
        Value::dict([
            ("Type", Value::name("Pages")),
            ("Kids", Value::array([Value::reference(page_id)])),
            ("Count", Value::Integer(1)),
        ]),
    )
    .expect("pages_id was just allocated");
    let catalog_id = w.add(Value::dict([
        ("Type", Value::name("Catalog")),
        ("Pages", Value::reference(pages_id)),
    ]));
    w.set_root(catalog_id);

    w.finish()
        .expect("typeset layout never produces non-finite coordinates")
}

/// Render every [`DrawCommand`] into PDF content-stream operators, applying
/// each command's own CTM to its path coordinates directly (so no `cm`
/// operator bookkeeping is needed in the stream itself).
fn content_stream(tree: &RenderTree) -> Vec<u8> {
    let mut out = Vec::new();
    for cmd in &tree.commands {
        match cmd {
            DrawCommand::Fill { path, color, ctm } => {
                write_color(&mut out, *color, "rg");
                write_path(&mut out, path, ctm);
                out.extend_from_slice(b"f\n");
            }
            DrawCommand::FillEvenOdd { path, color, ctm } => {
                write_color(&mut out, *color, "rg");
                write_path(&mut out, path, ctm);
                out.extend_from_slice(b"f*\n");
            }
            DrawCommand::Stroke {
                path,
                color,
                line_width,
                ctm,
            } => {
                write_color(&mut out, *color, "RG");
                write_num(&mut out, *line_width);
                out.extend_from_slice(b" w\n");
                write_path(&mut out, path, ctm);
                out.extend_from_slice(b"S\n");
            }
            DrawCommand::Clip { path, ctm } => {
                write_path(&mut out, path, ctm);
                out.extend_from_slice(b"W n\n");
            }
        }
    }
    out
}

fn write_color(out: &mut Vec<u8>, color: RgbColor, op: &str) {
    write_num(out, color.r);
    out.push(b' ');
    write_num(out, color.g);
    out.push(b' ');
    write_num(out, color.b);
    out.push(b' ');
    out.extend_from_slice(op.as_bytes());
    out.push(b'\n');
}

fn write_path(out: &mut Vec<u8>, path: &Path, ctm: &Transform) {
    for sub in &path.subpaths {
        write_point(out, ctm.apply(sub.start));
        out.extend_from_slice(b" m\n");
        for seg in &sub.segments {
            write_point(out, ctm.apply(seg.control1));
            out.push(b' ');
            write_point(out, ctm.apply(seg.control2));
            out.push(b' ');
            write_point(out, ctm.apply(seg.end));
            out.extend_from_slice(b" c\n");
        }
        if sub.closed {
            out.extend_from_slice(b"h\n");
        }
    }
}

fn write_point(out: &mut Vec<u8>, p: Point) {
    write_num(out, p.x);
    out.push(b' ');
    write_num(out, p.y);
}

fn write_num(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(format!("{v:.3}").as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_glyph_math::ast::FractionBar;

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
    fn produces_a_well_formed_pdf() {
        let font = sample_font();
        let bytes = render_math_to_pdf(&spec2_example(), &font, 40.0);
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
        assert!(bytes.windows(9).any(|w| w == b"/Type /Pa"));
    }

    #[test]
    fn parses_from_latex_string() {
        let expr = tpt_glyph_math::latex::parse(r"\frac{x}{y^2}").unwrap();
        assert_eq!(expr, spec2_example());
    }
}
