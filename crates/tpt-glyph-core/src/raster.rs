// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / raster
//
// A self-contained software rasterizer. It consumes a `RenderTree` (the
// backend-agnostic command stream produced by the operator pipeline) and emits
// pixels into a `Canvas`.
//
// The scanline algorithm used here is fully deterministic and holds no global
// mutable state, so it is safe to run concurrently across pages on a rayon
// pool. The `wgpu`/`raqote` backends (Phase 6) implement the same `Rasterizer`
// trait for GPU/CPU acceleration; this module is the always available reference
// backend.

use crate::canvas::{Canvas, Pixel};
use crate::error::Result;
use crate::geometry::{CubicBezier, Path, Point, Transform};
use crate::graphics_state::{LineCap, LineJoin, RgbColor};
#[cfg(not(feature = "std"))]
use crate::math::F64MathExt;
use crate::render::{DrawCommand, RenderTree};
use alloc::vec::Vec;

/// Maximum recursion depth when flattening Bézier curves. Guards against
/// pathological inputs without a global quota.
const MAX_FLATTEN_DEPTH: u32 = 24;

/// Flatten a cubic Bézier into a polyline of device-space points.
fn flatten_curve(curve: &CubicBezier, depth: u32, out: &mut Vec<Point>) {
    let flat_enough = |c: &CubicBezier| -> bool {
        let d1 = c.control1 - c.start;
        let d2 = c.control2 - c.control1;
        let d3 = c.end - c.control2;
        let cross = (d1.x * d2.y - d1.y * d2.x).abs() + (d2.x * d3.y - d2.y * d3.x).abs();
        let len = (d1.x * d1.x + d1.y * d1.y).sqrt()
            + (d2.x * d2.x + d2.y * d2.y).sqrt()
            + (d3.x * d3.x + d3.y * d3.y).sqrt();
        cross <= 1e-3 * len.max(1.0)
    };

    if depth >= MAX_FLATTEN_DEPTH || flat_enough(curve) {
        out.push(curve.end);
        return;
    }
    // de Casteljau subdivision at t = 0.5.
    let m01 = midpoint(curve.start, curve.control1);
    let m12 = midpoint(curve.control1, curve.control2);
    let m23 = midpoint(curve.control2, curve.end);
    let m012 = midpoint(m01, m12);
    let m123 = midpoint(m12, m23);
    let m = midpoint(m012, m123);
    flatten_curve(
        &CubicBezier {
            start: curve.start,
            control1: m01,
            control2: m012,
            end: m,
        },
        depth + 1,
        out,
    );
    flatten_curve(
        &CubicBezier {
            start: m,
            control1: m123,
            control2: m23,
            end: curve.end,
        },
        depth + 1,
        out,
    );
}

fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

/// Flatten an entire path into device-space polylines, one per subpath. Closed
/// subpaths are closed explicitly so fill algorithms see a loop.
fn flatten_path(path: &Path, ctm: &Transform) -> Vec<Vec<Point>> {
    let mut result = Vec::with_capacity(path.subpaths.len());
    for sub in &path.subpaths {
        let mut pts = Vec::new();
        pts.push(ctm.apply(sub.start));
        for seg in &sub.segments {
            let world = CubicBezier {
                start: ctm.apply(seg.start),
                control1: ctm.apply(seg.control1),
                control2: ctm.apply(seg.control2),
                end: ctm.apply(seg.end),
            };
            flatten_curve(&world, 0, &mut pts);
        }
        if sub.closed && pts.len() >= 2 {
            pts.push(pts[0]);
        }
        result.push(pts);
    }
    result
}

fn to_pixel(color: RgbColor) -> Pixel {
    Pixel {
        r: (color.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        g: (color.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        b: (color.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        a: 255,
    }
}

/// Authoritative scanline fill that writes into `pixels`.
///
/// `even_odd` selects the PostScript `eofill` rule; otherwise the non-zero
/// winding rule is used (PostScript `fill`).
/// A mutable rasterization target: a pixel buffer with its device dimensions.
struct RasterTarget<'a> {
    width: u32,
    height: u32,
    pixels: &'a mut [Pixel],
}

impl RasterTarget<'_> {
    fn write(&mut self, x: u32, y: u32, pixel: Pixel) {
        if x < self.width && y < self.height {
            let idx = y as usize * self.width as usize + x as usize;
            if idx < self.pixels.len() {
                self.pixels[idx] = pixel;
            }
        }
    }
}

fn scanline_fill_into(
    target: &mut RasterTarget<'_>,
    polygons: &[Vec<Point>],
    pixel: Pixel,
    even_odd: bool,
) {
    if target.width == 0 || target.height == 0 {
        return;
    }
    for y in 0..target.height {
        let fy = y as f64 + 0.5;
        // Collect crossing x-positions and the edge direction (for winding).
        let mut edges: Vec<(f64, f64)> = Vec::new(); // (x, dir)
        for poly in polygons {
            let n = poly.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let (a, b) = if i + 1 < n {
                    (poly[i], poly[i + 1])
                } else {
                    (poly[n - 1], poly[0])
                };
                let (y0, y1) = (a.y, b.y);
                let crosses = (y0 <= fy && fy < y1) || (y1 <= fy && fy < y0);
                if crosses {
                    let t = (fy - y0) / (y1 - y0);
                    let x = a.x + t * (b.x - a.x);
                    let dir = if y1 > y0 { 1.0 } else { -1.0 };
                    edges.push((x, dir));
                }
            }
        }
        if edges.is_empty() {
            continue;
        }
        edges.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(core::cmp::Ordering::Equal));

        if even_odd {
            let mut i = 0;
            while i + 1 < edges.len() {
                fill_span(target, y, edges[i].0, edges[i + 1].0, pixel);
                i += 2;
            }
        } else {
            let mut winding = 0i32;
            for k in 0..edges.len() {
                let x = edges[k].0;
                if winding != 0 {
                    let prev = edges[k - 1].0;
                    fill_span(target, y, prev, x, pixel);
                }
                winding += edges[k].1 as i32;
            }
        }
    }
}

fn fill_span(target: &mut RasterTarget<'_>, y: u32, x0: f64, x1: f64, pixel: Pixel) {
    let x_start = x0.max(0.0).ceil() as i64;
    let x_end = (x1 - 1.0).max(0.0).floor() as i64;
    if x_start > x_end {
        return;
    }
    for x in x_start..=x_end {
        if x < 0 || x >= target.width as i64 {
            continue;
        }
        target.write(x as u32, y, pixel);
    }
}

/// Stroke a path by expanding each segment into a thick rectangle of width
/// `line_width` and filling the union. Caps are honored; joins are approximated
/// by overlapping rectangles, which is visually acceptable for the reference
/// backend.
fn rasterize_stroke(
    target: &mut RasterTarget<'_>,
    polygons: &[Vec<Point>],
    pixel: Pixel,
    line_width: f64,
    cap: LineCap,
    _join: LineJoin,
) {
    if line_width <= 0.0 {
        return;
    }
    let hw = line_width / 2.0;
    let mut rects: Vec<Vec<Point>> = Vec::new();
    for poly in polygons {
        let n = poly.len();
        for i in 0..n.saturating_sub(1) {
            let a = poly[i];
            let b = poly[i + 1];
            if a == b {
                continue;
            }
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len == 0.0 {
                continue;
            }
            let nx = -dy / len * hw;
            let ny = dx / len * hw;
            rects.push(vec![
                Point::new(a.x + nx, a.y + ny),
                Point::new(b.x + nx, b.y + ny),
                Point::new(b.x - nx, b.y - ny),
                Point::new(a.x - nx, a.y - ny),
            ]);
        }
        if let (Some(first), Some(last)) = (poly.first().copied(), poly.last().copied()) {
            if first != last {
                add_cap_to(&mut rects, first, poly.get(1).copied(), hw, cap);
                let plen = poly.len();
                add_cap_to(
                    &mut rects,
                    last,
                    poly.get(plen.wrapping_sub(2)).copied(),
                    hw,
                    cap,
                );
            }
        }
    }
    scanline_fill_into(target, &rects, pixel, true);
}

fn add_cap_to(
    rects: &mut Vec<Vec<Point>>,
    p: Point,
    neighbor: Option<Point>,
    hw: f64,
    cap: LineCap,
) {
    let dir = match neighbor {
        Some(n) => {
            let dx = p.x - n.x;
            let dy = p.y - n.y;
            let l = (dx * dx + dy * dy).sqrt();
            if l == 0.0 {
                return;
            }
            (dx / l, dy / l)
        }
        None => return,
    };
    match cap {
        LineCap::Butt => {}
        LineCap::Round => {
            let steps = 8usize;
            let mut pts = Vec::with_capacity(steps + 1);
            for s in 0..=steps {
                let ang = (s as f64 / steps as f64) * core::f64::consts::PI;
                let ox = dir.0 * hw * ang.cos() - dir.1 * hw * ang.sin();
                let oy = dir.0 * hw * ang.sin() + dir.1 * hw * ang.cos();
                pts.push(Point::new(p.x + ox, p.y + oy));
            }
            if pts.len() >= 3 {
                rects.push(pts);
            }
        }
        LineCap::ProjectingSquare => {
            let ex = dir.0 * hw;
            let ey = dir.1 * hw;
            let nx = -dir.1 * hw;
            let ny = dir.0 * hw;
            rects.push(vec![
                Point::new(p.x + nx + ex, p.y + ny + ey),
                Point::new(p.x - nx + ex, p.y - ny + ey),
                Point::new(p.x - nx - ex, p.y - ny - ey),
                Point::new(p.x + nx - ex, p.y + ny - ey),
            ]);
        }
    }
}

/// The reference software rasterizer backend.
pub struct SoftwareRasterizer;

impl SoftwareRasterizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SoftwareRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::render::Rasterizer for SoftwareRasterizer {
    fn rasterize(&self, tree: &RenderTree) -> Result<Canvas> {
        let mut canvas = Canvas::new(tree.width, tree.height, Pixel::WHITE);
        let mut target = RasterTarget {
            width: tree.width,
            height: tree.height,
            pixels: &mut canvas.pixels,
        };

        for cmd in &tree.commands {
            match cmd {
                DrawCommand::Clip { .. } => {
                    // Clip geometry is recorded by full backends; the reference
                    // backend treats clip as a no-op region (identity).
                }
                DrawCommand::Fill { path, color, ctm } => {
                    let poly = flatten_path(path, ctm);
                    scanline_fill_into(&mut target, &poly, to_pixel(*color), false);
                }
                DrawCommand::FillEvenOdd { path, color, ctm } => {
                    let poly = flatten_path(path, ctm);
                    scanline_fill_into(&mut target, &poly, to_pixel(*color), true);
                }
                DrawCommand::Stroke {
                    path,
                    color,
                    line_width,
                    ctm,
                } => {
                    let poly = flatten_path(path, ctm);
                    rasterize_stroke(
                        &mut target,
                        &poly,
                        to_pixel(*color),
                        *line_width,
                        LineCap::Butt,
                        LineJoin::Miter,
                    );
                }
            }
        }
        Ok(canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, SelectedBackend};
    use crate::geometry::{CubicBezier, Path, Subpath};
    use crate::render::Rasterizer;

    use crate::graphics_state::{GraphicsState, RgbColor};
    use crate::render::RenderTree;

    #[test]
    fn fills_a_unit_square_with_color() {
        let mut tree = RenderTree::new(32, 32);
        let state = GraphicsState::new().with_fill_color(RgbColor::new(1.0, 0.0, 0.0));
        let path = Path {
            subpaths: vec![Subpath {
                start: Point::new(4.0, 4.0),
                segments: vec![
                    CubicBezier {
                        start: Point::new(4.0, 4.0),
                        control1: Point::new(28.0, 4.0),
                        control2: Point::new(28.0, 4.0),
                        end: Point::new(28.0, 4.0),
                    },
                    CubicBezier {
                        start: Point::new(28.0, 4.0),
                        control1: Point::new(28.0, 28.0),
                        control2: Point::new(28.0, 28.0),
                        end: Point::new(28.0, 28.0),
                    },
                    CubicBezier {
                        start: Point::new(28.0, 28.0),
                        control1: Point::new(4.0, 28.0),
                        control2: Point::new(4.0, 28.0),
                        end: Point::new(4.0, 28.0),
                    },
                    CubicBezier {
                        start: Point::new(4.0, 28.0),
                        control1: Point::new(4.0, 4.0),
                        control2: Point::new(4.0, 4.0),
                        end: Point::new(4.0, 4.0),
                    },
                ],
                closed: true,
            }],
        };
        tree.fill(&state, path);
        let canvas = SoftwareRasterizer.rasterize(&tree).unwrap();
        let reds: usize = canvas
            .pixels
            .iter()
            .filter(|p| p.r > 200 && p.g < 50)
            .count();
        assert!(
            reds > 100,
            "expected a filled red region, got {reds} red px"
        );
    }

    #[test]
    fn strokes_a_diagonal_line() {
        let mut tree = RenderTree::new(32, 32);
        let state = GraphicsState::new()
            .with_stroke_color(RgbColor::new(0.0, 0.0, 1.0))
            .with_line_width(3.0);
        let path = Path {
            subpaths: vec![Subpath {
                start: Point::new(2.0, 2.0),
                segments: vec![CubicBezier {
                    start: Point::new(2.0, 2.0),
                    control1: Point::new(30.0, 30.0),
                    control2: Point::new(30.0, 30.0),
                    end: Point::new(30.0, 30.0),
                }],
                closed: false,
            }],
        };
        tree.stroke(&state, path);
        let canvas = SoftwareRasterizer.rasterize(&tree).unwrap();
        let blues: usize = canvas
            .pixels
            .iter()
            .filter(|p| p.b > 200 && p.r < 50)
            .count();
        assert!(blues > 20, "expected a blue stroke, got {blues} blue px");
    }

    #[test]
    fn backend_selection_resolves_to_a_cpu_backend() {
        // Auto-selection prefers the raqote backend when compiled in, otherwise
        // the reference software backend — both are CPU backends.
        let kind = SelectedBackend::auto(None).kind();
        assert!(matches!(kind, Backend::Cpu | Backend::CpuRaqote));
    }

    #[cfg(feature = "raqote-backend")]
    #[test]
    fn backend_selection_prefers_raqote_when_available() {
        assert_eq!(SelectedBackend::auto(None).kind(), Backend::CpuRaqote);
    }
}
