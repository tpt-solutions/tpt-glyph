// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-core / backends / raqote
//
// A CPU rasterization backend backed by `raqote`. This is the deterministic,
// dependency-light fallback path used when no GPU adapter is available. It
// implements the same `Rasterizer` trait as the reference software rasterizer,
// so it is a drop-in for `SelectedBackend`.
//
// raqote is gated behind the `raqote-backend` feature so that the always-available
// reference backend remains the default build.

use crate::canvas::Canvas;
use crate::error::Result;
use crate::geometry::{Path, Point, Transform};
use crate::graphics_state::{LineCap, LineJoin, RgbColor};
use crate::render::{DrawCommand, Rasterizer, RenderTree};

/// Rasterizer that delegates to `raqote`.
pub struct RaqoteRasterizer;

impl RaqoteRasterizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RaqoteRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

fn to_raqote_color(c: RgbColor) -> raqote::Color {
    raqote::Color::new(
        255,
        (c.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.b.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn cap_style(cap: LineCap) -> raqote::LineCap {
    match cap {
        LineCap::Butt => raqote::LineCap::Butt,
        LineCap::Round => raqote::LineCap::Round,
        LineCap::ProjectingSquare => raqote::LineCap::Square,
    }
}

fn join_style(join: LineJoin) -> raqote::LineJoin {
    match join {
        LineJoin::Miter => raqote::LineJoin::Miter,
        LineJoin::Round => raqote::LineJoin::Round,
        LineJoin::Bevel => raqote::LineJoin::Bevel,
    }
}

/// Flatten a path into `raqote::PathBuilder` space, applying `ctm`, and set the
/// requested fill winding rule.
fn build_raqote_path(
    mut pb: raqote::PathBuilder,
    path: &Path,
    ctm: &Transform,
    winding: raqote::Winding,
) -> raqote::Path {
    for sub in &path.subpaths {
        let start = ctm.apply(sub.start);
        pb.move_to(start.x as f32, start.y as f32);
        for seg in &sub.segments {
            let c1 = ctm.apply(seg.control1);
            let c2 = ctm.apply(seg.control2);
            let end = ctm.apply(seg.end);
            pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                end.x as f32,
                end.y as f32,
            );
        }
        if sub.closed {
            pb.close();
        }
    }
    let mut p = pb.finish();
    p.winding = winding;
    p
}

impl Rasterizer for RaqoteRasterizer {
    fn rasterize(&self, tree: &RenderTree) -> Result<Canvas> {
        let mut dt = raqote::DrawTarget::new(tree.width as i32, tree.height as i32);
        dt.clear(raqote::Color::new(255, 255, 255, 255).into());

        for cmd in &tree.commands {
            match cmd {
                DrawCommand::Clip { .. } => {
                    // Clip regions are recorded but not yet intersected here; the
                    // reference backend treats clip as identity and so does this one.
                }
                DrawCommand::Fill { path, color, ctm } => {
                    paint_fill(&mut dt, path, ctm, *color, raqote::Winding::NonZero)
                }
                DrawCommand::FillEvenOdd { path, color, ctm } => {
                    paint_fill(&mut dt, path, ctm, *color, raqote::Winding::EvenOdd)
                }
                DrawCommand::Stroke {
                    path,
                    color,
                    line_width,
                    ctm,
                } => {
                    let pb = raqote::PathBuilder::new();
                    let p = build_raqote_path(pb, path, ctm, raqote::Winding::NonZero);
                    let style = raqote::StrokeStyle {
                        width: *line_width as f32,
                        cap: cap_style(LineCap::Butt),
                        join: join_style(LineJoin::Miter),
                        ..raqote::StrokeStyle::default()
                    };
                    dt.stroke(
                        &p,
                        &raqote::Source::from(to_raqote_color(*color)),
                        &style,
                        &raqote::DrawOptions::new(),
                    );
                }
            }
        }

        let mut canvas = Canvas::new(tree.width, tree.height, crate::canvas::Pixel::WHITE);
        // raqote's buffer is premultiplied 0xAARRGGBB; unpremultiply back to
        // straight alpha so downstream consumers (PNG export, diffing) match.
        for (i, px) in canvas.pixels.iter_mut().enumerate() {
            let d = dt.get_data()[i];
            let a = (d >> 24) & 0xff;
            let r = (d >> 16) & 0xff;
            let g = (d >> 8) & 0xff;
            let b = d & 0xff;
            let (r, g, b) = if a == 0 {
                (0u8, 0u8, 0u8)
            } else {
                let inv = 255.0 / a as f64;
                (
                    (r as f64 * inv).round() as u8,
                    (g as f64 * inv).round() as u8,
                    (b as f64 * inv).round() as u8,
                )
            };
            px.r = r;
            px.g = g;
            px.b = b;
            px.a = a as u8;
        }
        Ok(canvas)
    }
}

fn paint_fill(
    dt: &mut raqote::DrawTarget,
    path: &Path,
    ctm: &Transform,
    color: RgbColor,
    winding: raqote::Winding,
) {
    let pb = raqote::PathBuilder::new();
    let p = build_raqote_path(pb, path, ctm, winding);
    dt.fill(
        &p,
        &raqote::Source::from(to_raqote_color(color)),
        &raqote::DrawOptions::new(),
    );
}

// Keep `Point` referenced so the import is meaningful for downstream reuse.
#[allow(dead_code)]
fn _assert_point(_p: Point) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{CubicBezier, Subpath};
    use crate::graphics_state::{GraphicsState, RgbColor};
    use crate::render::RenderTree;

    /// Build a small tree with a red filled square and a blue diagonal stroke.
    fn sample_tree() -> RenderTree {
        let mut tree = RenderTree::new(40, 40);
        let fill_state = GraphicsState::new().with_fill_color(RgbColor::new(1.0, 0.0, 0.0));
        let stroke_state = GraphicsState::new()
            .with_stroke_color(RgbColor::new(0.0, 0.0, 1.0))
            .with_line_width(3.0);
        tree.fill(&fill_state, square(6.0, 34.0));
        tree.stroke(&stroke_state, diagonal());
        tree
    }

    fn square(lo: f64, hi: f64) -> Path {
        Path {
            subpaths: vec![Subpath {
                start: Point::new(lo, lo),
                segments: vec![hline(lo, hi), vline(hi, hi), hline(hi, lo), vline(lo, lo)],
                closed: true,
            }],
        }
    }

    fn hline(x0: f64, x1: f64) -> CubicBezier {
        CubicBezier {
            start: Point::new(x0, 0.0),
            control1: Point::new((x0 + x1) / 2.0, 0.0),
            control2: Point::new((x0 + x1) / 2.0, 0.0),
            end: Point::new(x1, 0.0),
        }
    }

    fn vline(y0: f64, y1: f64) -> CubicBezier {
        CubicBezier {
            start: Point::new(0.0, y0),
            control1: Point::new(0.0, (y0 + y1) / 2.0),
            control2: Point::new(0.0, (y0 + y1) / 2.0),
            end: Point::new(0.0, y1),
        }
    }

    fn diagonal() -> Path {
        Path {
            subpaths: vec![Subpath {
                start: Point::new(4.0, 4.0),
                segments: vec![CubicBezier {
                    start: Point::new(4.0, 4.0),
                    control1: Point::new(36.0, 36.0),
                    control2: Point::new(36.0, 36.0),
                    end: Point::new(36.0, 36.0),
                }],
                closed: false,
            }],
        }
    }

    #[test]
    fn raqote_matches_reference_backend() {
        let tree = sample_tree();
        let reference = crate::raster::SoftwareRasterizer.rasterize(&tree).unwrap();
        let raqote = RaqoteRasterizer.rasterize(&tree).unwrap();

        assert_eq!(reference.width, raqote.width);
        assert_eq!(reference.height, raqote.height);

        // Both backends must produce clearly non-empty output with the expected
        // dominant colors present.
        let is_red = |p: &crate::canvas::Pixel| p.r > 200 && p.g < 50;
        let is_blue = |p: &crate::canvas::Pixel| p.b > 200 && p.r < 50;
        assert!(reference.pixels.iter().filter(|p| is_red(p)).count() > 100);
        assert!(raqote.pixels.iter().filter(|p| is_red(p)).count() > 100);
        assert!(reference.pixels.iter().filter(|p| is_blue(p)).count() > 20);
        assert!(raqote.pixels.iter().filter(|p| is_blue(p)).count() > 20);

        // The reference backend is hard-aliased while raqote antialiases, so edge
        // pixels differ in intensity. They must still agree closely: count pixels
        // whose RGB differs from the other backend by more than 60/255 on any
        // channel and require that they are a small minority (edges only).
        let mut diverged = 0usize;
        for (a, b) in reference.pixels.iter().zip(raqote.pixels.iter()) {
            let dr = (a.r as i32 - b.r as i32).abs();
            let dg = (a.g as i32 - b.g as i32).abs();
            let db = (a.b as i32 - b.b as i32).abs();
            let da = (a.a as i32 - b.a as i32).abs();
            if dr > 60 || dg > 60 || db > 60 || da > 60 {
                diverged += 1;
            }
        }
        let frac = diverged as f64 / reference.pixels.len() as f64;
        assert!(
            frac < 0.25,
            "backends diverged on {frac:.2} of pixels (expected edges only)"
        );
    }
}
