// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-measure / geometry
//
// Interprets a page's content-stream operators directly into flattened,
// CTM-transformed polygon/polyline geometry — the same "painted path"
// concept as the rendering pipeline's `DrawCommand`, but computed without a
// rasterizer so this crate has no dependency on `tpt-glyph-core`.

use tpt_glyph_pdf_ir::{ContentStream, Matrix, Operation, Page, Rect};

/// How a painted path was consumed by the content stream that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintKind {
    Fill,
    FillEvenOdd,
    Stroke,
    FillAndStroke,
}

/// A single painted path: its paint kind and its flattened subpaths, each a
/// polyline in the page's own user-space units (PDF points, y increasing
/// upward — no device/canvas mapping is applied).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedPath {
    pub kind: PaintKind,
    pub subpaths: Vec<Vec<(f64, f64)>>,
}

/// Extract every painted (filled/stroked) path on `page`, across all of its
/// content streams, as flattened polylines in page user-space units.
///
/// Text-showing and XObject-painting operators are not walked; this
/// measures vector path geometry only (see the crate-level docs).
pub fn painted_paths(page: &Page) -> Vec<PaintedPath> {
    let mut out = Vec::new();
    for stream in &page.contents {
        walk_content_stream(stream, &mut out);
    }
    out
}

/// The bounding box of a set of painted paths, in page user-space units.
/// `None` if `paths` contains no points at all.
pub fn bounding_box(paths: &[PaintedPath]) -> Option<Rect> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;
    for path in paths {
        for sub in &path.subpaths {
            for &(x, y) in sub {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    found.then(|| Rect::new(min_x, min_y, max_x, max_y))
}

/// The total polyline length of a set of painted paths (sum over every
/// subpath's consecutive-point segment lengths, including the closing edge
/// for subpaths whose first and last points differ).
pub fn total_length(paths: &[PaintedPath]) -> f64 {
    let mut total = 0.0;
    for path in paths {
        for sub in &path.subpaths {
            for w in sub.windows(2) {
                total += distance(w[0], w[1]);
            }
        }
    }
    total
}

/// Estimate the fraction of `page`'s media box covered by filled path ink,
/// in `[0.0, 1.0]`.
///
/// This sums each filled subpath's polygon area (the shoelace formula) and
/// divides by the page area. It is an *estimate*: overlapping fills are
/// double-counted rather than unioned, and stroked-only paint and text are
/// not included (see the crate-level docs for scope).
pub fn ink_coverage(page: &Page) -> f64 {
    let page_area = page.media_box.width() * page.media_box.height();
    if page_area <= 0.0 {
        return 0.0;
    }
    let mut ink = 0.0;
    for path in painted_paths(page) {
        if !matches!(
            path.kind,
            PaintKind::Fill | PaintKind::FillEvenOdd | PaintKind::FillAndStroke
        ) {
            continue;
        }
        for sub in &path.subpaths {
            ink += polygon_area(sub);
        }
    }
    (ink / page_area).clamp(0.0, 1.0)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    (dx * dx + dy * dy).sqrt()
}

/// Unsigned polygon area via the shoelace formula. Works on an open polyline
/// too (treated as implicitly closed), which is the right behavior for a
/// filled path: PDF fill operators implicitly close every subpath.
fn polygon_area(points: &[(f64, f64)]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % points.len()];
        sum += x0 * y1 - x1 * y0;
    }
    (sum * 0.5).abs()
}

// ---------------------------------------------------------------------------
// Content-stream walking
// ---------------------------------------------------------------------------

/// A subpath under construction: raw (pre-CTM) coordinates as given by the
/// content stream's path-construction operators.
struct RawSubpath {
    start: (f64, f64),
    segments: Vec<RawSegment>,
}

enum RawSegment {
    Line((f64, f64)),
    Cubic {
        c1: (f64, f64),
        c2: (f64, f64),
        end: (f64, f64),
    },
}

impl RawSubpath {
    fn current_point(&self) -> (f64, f64) {
        match self.segments.last() {
            Some(RawSegment::Line(p)) => *p,
            Some(RawSegment::Cubic { end, .. }) => *end,
            None => self.start,
        }
    }
}

fn walk_content_stream(stream: &ContentStream, out: &mut Vec<PaintedPath>) {
    let mut ctm_stack: Vec<Matrix> = vec![Matrix::IDENTITY];
    let mut path: Vec<RawSubpath> = Vec::new();

    macro_rules! ctm {
        () => {
            *ctm_stack.last().expect("ctm stack always has a base entry")
        };
    }

    for op in &stream.ops {
        match op {
            Operation::Save => ctm_stack.push(ctm!()),
            Operation::Restore => {
                if ctm_stack.len() > 1 {
                    ctm_stack.pop();
                }
            }
            Operation::ConcatMatrix(m) => {
                let new_ctm = concat(m, &ctm!());
                *ctm_stack.last_mut().expect("non-empty") = new_ctm;
            }
            Operation::MoveTo(x, y) => path.push(RawSubpath {
                start: (*x, *y),
                segments: Vec::new(),
            }),
            Operation::LineTo(x, y) => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Line((*x, *y)));
                }
            }
            Operation::CurveTo(x1, y1, x2, y2, x3, y3) => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Cubic {
                        c1: (*x1, *y1),
                        c2: (*x2, *y2),
                        end: (*x3, *y3),
                    });
                }
            }
            Operation::CurveToV(x2, y2, x3, y3) => {
                if let Some(sp) = path.last_mut() {
                    let c1 = sp.current_point();
                    sp.segments.push(RawSegment::Cubic {
                        c1,
                        c2: (*x2, *y2),
                        end: (*x3, *y3),
                    });
                }
            }
            Operation::CurveToY(x1, y1, x3, y3) => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Cubic {
                        c1: (*x1, *y1),
                        c2: (*x3, *y3),
                        end: (*x3, *y3),
                    });
                }
            }
            Operation::Rectangle(x, y, w, h) => {
                let mut sp = RawSubpath {
                    start: (*x, *y),
                    segments: Vec::new(),
                };
                sp.segments.push(RawSegment::Line((x + w, *y)));
                sp.segments.push(RawSegment::Line((x + w, y + h)));
                sp.segments.push(RawSegment::Line((*x, y + h)));
                sp.segments.push(RawSegment::Line((*x, *y)));
                path.push(sp);
            }
            Operation::CloseSubpath => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Line(sp.start));
                }
            }
            Operation::Fill => {
                record(&path, &ctm!(), PaintKind::Fill, out);
                path.clear();
            }
            Operation::FillEvenOdd => {
                record(&path, &ctm!(), PaintKind::FillEvenOdd, out);
                path.clear();
            }
            Operation::Stroke => {
                record(&path, &ctm!(), PaintKind::Stroke, out);
                path.clear();
            }
            Operation::CloseAndStroke => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Line(sp.start));
                }
                record(&path, &ctm!(), PaintKind::Stroke, out);
                path.clear();
            }
            Operation::FillAndStroke | Operation::FillAndStrokeEvenOdd => {
                record(&path, &ctm!(), PaintKind::FillAndStroke, out);
                path.clear();
            }
            Operation::CloseFillAndStroke | Operation::CloseFillAndStrokeEvenOdd => {
                if let Some(sp) = path.last_mut() {
                    sp.segments.push(RawSegment::Line(sp.start));
                }
                record(&path, &ctm!(), PaintKind::FillAndStroke, out);
                path.clear();
            }
            Operation::EndPath => {
                // `n` ends the path without painting ink.
                path.clear();
            }
            Operation::Clip | Operation::ClipEvenOdd => {
                // `W`/`W*` mark the path for clipping but do not themselves
                // terminate it — the path-painting operator that follows
                // (commonly `n`, but a fill/stroke may clip *and* paint in
                // the same operator) still consumes it as usual. Clip
                // regions aren't part of this crate's measured geometry, so
                // there's nothing to record here.
            }
            _ => {}
        }
    }
}

fn record(path: &[RawSubpath], ctm: &Matrix, kind: PaintKind, out: &mut Vec<PaintedPath>) {
    if path.is_empty() {
        return;
    }
    let subpaths = path.iter().map(|sp| flatten_subpath(sp, ctm)).collect();
    out.push(PaintedPath { kind, subpaths });
}

fn flatten_subpath(sp: &RawSubpath, ctm: &Matrix) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let mut cur = sp.start;
    pts.push(apply(ctm, cur));
    for seg in &sp.segments {
        match seg {
            RawSegment::Line(p) => {
                pts.push(apply(ctm, *p));
                cur = *p;
            }
            RawSegment::Cubic { c1, c2, end } => {
                flatten_cubic(
                    apply(ctm, cur),
                    apply(ctm, *c1),
                    apply(ctm, *c2),
                    apply(ctm, *end),
                    0,
                    &mut pts,
                );
                cur = *end;
            }
        }
    }
    pts
}

/// Adaptively flatten a cubic Bézier (already in target/device coordinates)
/// into line segments, appending points after `start` (which the caller has
/// already pushed) to `out`. Recursion is capped so adversarial/degenerate
/// curves can't cause unbounded work.
fn flatten_cubic(
    start: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    end: (f64, f64),
    depth: u32,
    out: &mut Vec<(f64, f64)>,
) {
    const MAX_DEPTH: u32 = 16;
    const FLATNESS: f64 = 0.2;

    if depth >= MAX_DEPTH || is_flat(start, c1, c2, end, FLATNESS) {
        out.push(end);
        return;
    }

    // De Casteljau subdivision at t = 0.5.
    let p01 = midpoint(start, c1);
    let p12 = midpoint(c1, c2);
    let p23 = midpoint(c2, end);
    let p012 = midpoint(p01, p12);
    let p123 = midpoint(p12, p23);
    let mid = midpoint(p012, p123);

    flatten_cubic(start, p01, p012, mid, depth + 1, out);
    flatten_cubic(mid, p123, p23, end, depth + 1, out);
}

fn midpoint(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Maximum perpendicular distance of the control points from the chord
/// `start`–`end`, used as the subdivision flatness test.
fn is_flat(
    start: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    end: (f64, f64),
    tolerance: f64,
) -> bool {
    point_line_distance(c1, start, end) <= tolerance
        && point_line_distance(c2, start, end) <= tolerance
}

fn point_line_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return distance(p, a);
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

// ---------------------------------------------------------------------------
// Matrix helpers (mirrors `tpt-glyph-core::geometry::Transform`, reimplemented
// locally so this crate has no rendering-pipeline dependency)
// ---------------------------------------------------------------------------

/// Apply `m` to a point: PDF/PostScript convention, `(x, y) -> (a·x + c·y +
/// e, b·x + d·y + f)`.
fn apply(m: &Matrix, p: (f64, f64)) -> (f64, f64) {
    (m.a * p.0 + m.c * p.1 + m.e, m.b * p.0 + m.d * p.1 + m.f)
}

/// Compose `self` after `other` (matrix product `self · other` in row-vector
/// form: apply `self` first, then `other`).
fn concat(this: &Matrix, other: &Matrix) -> Matrix {
    Matrix {
        a: this.a * other.a + this.b * other.c,
        b: this.a * other.b + this.b * other.d,
        c: this.c * other.a + this.d * other.c,
        d: this.c * other.b + this.d * other.d,
        e: this.e * other.a + this.f * other.c + other.e,
        f: this.e * other.b + this.f * other.d + other.f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_glyph_pdf_ir::{Rect, Resources};

    fn page_with(ops: Vec<Operation>) -> Page {
        Page {
            index: 0,
            label: None,
            media_box: Rect::new(0.0, 0.0, 200.0, 200.0),
            crop_box: None,
            bleed_box: None,
            trim_box: None,
            art_box: None,
            rotate: 0,
            resources: Resources::empty(),
            contents: vec![ContentStream::new(ops)],
            annotations: Vec::new(),
            thumb: None,
            struct_parents: None,
        }
    }

    #[test]
    fn rectangle_bounding_box_matches_geometry() {
        let page = page_with(vec![
            Operation::Rectangle(10.0, 20.0, 80.0, 60.0),
            Operation::Fill,
        ]);
        let paths = painted_paths(&page);
        assert_eq!(paths.len(), 1);
        let bbox = bounding_box(&paths).unwrap();
        assert!((bbox.left - 10.0).abs() < 1e-9);
        assert!((bbox.bottom - 20.0).abs() < 1e-9);
        assert!((bbox.right - 90.0).abs() < 1e-9);
        assert!((bbox.top - 80.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_ink_coverage_matches_area_fraction() {
        let page = page_with(vec![
            Operation::Rectangle(0.0, 0.0, 100.0, 50.0),
            Operation::Fill,
        ]);
        // Page is 200x200 = 40_000; rect is 100x50 = 5_000 -> 12.5% coverage.
        let coverage = ink_coverage(&page);
        assert!((coverage - 0.125).abs() < 1e-9, "coverage was {coverage}");
    }

    #[test]
    fn line_length_matches_straight_distance() {
        let page = page_with(vec![
            Operation::MoveTo(0.0, 0.0),
            Operation::LineTo(30.0, 40.0),
            Operation::Stroke,
        ]);
        let paths = painted_paths(&page);
        assert_eq!(paths.len(), 1);
        assert!(matches!(paths[0].kind, PaintKind::Stroke));
        let length = total_length(&paths);
        assert!((length - 50.0).abs() < 1e-9, "length was {length}"); // 3-4-5 triangle
    }

    #[test]
    fn concat_matrix_scales_subsequent_geometry() {
        // Scale by 2x, then draw a rectangle in the now-scaled user space.
        let page = page_with(vec![
            Operation::ConcatMatrix(Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0)),
            Operation::Rectangle(0.0, 0.0, 10.0, 10.0),
            Operation::Fill,
        ]);
        let paths = painted_paths(&page);
        let bbox = bounding_box(&paths).unwrap();
        assert!(
            (bbox.width() - 20.0).abs() < 1e-9,
            "width was {}",
            bbox.width()
        );
    }

    #[test]
    fn save_restore_scopes_the_ctm() {
        let page = page_with(vec![
            Operation::Save,
            Operation::ConcatMatrix(Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0)),
            Operation::Rectangle(0.0, 0.0, 10.0, 10.0),
            Operation::Fill,
            Operation::Restore,
            // Back to the identity CTM: an identical rectangle should now be
            // half the size of the one drawn inside the save/restore scope.
            Operation::Rectangle(0.0, 0.0, 10.0, 10.0),
            Operation::Fill,
        ]);
        let paths = painted_paths(&page);
        assert_eq!(paths.len(), 2);
        let first_bbox = bounding_box(&paths[..1]).unwrap();
        let second_bbox = bounding_box(&paths[1..]).unwrap();
        assert!((first_bbox.width() - 20.0).abs() < 1e-9);
        assert!((second_bbox.width() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn curve_flattens_to_a_reasonable_polyline() {
        let page = page_with(vec![
            Operation::MoveTo(0.0, 0.0),
            Operation::CurveTo(0.0, 50.0, 50.0, 50.0, 50.0, 0.0),
            Operation::Fill,
        ]);
        let paths = painted_paths(&page);
        assert_eq!(paths.len(), 1);
        // A quarter-circle-ish bulge must flatten to more than just the two
        // endpoints, and every point must stay within the curve's bounding box.
        let sub = &paths[0].subpaths[0];
        assert!(
            sub.len() > 3,
            "expected multiple flattened points, got {}",
            sub.len()
        );
        for &(x, y) in sub {
            assert!((-1e-6..=50.0 + 1e-6).contains(&x));
            assert!((-1e-6..=50.0 + 1e-6).contains(&y));
        }
    }

    #[test]
    fn end_path_without_painting_produces_no_geometry() {
        let page = page_with(vec![
            Operation::Rectangle(0.0, 0.0, 10.0, 10.0),
            Operation::Clip,
            Operation::EndPath,
        ]);
        assert!(painted_paths(&page).is_empty());
    }
}
