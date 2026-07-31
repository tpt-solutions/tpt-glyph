// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / geometry
//
// Core 2D geometry primitives: points, transforms, subpaths, and Bézier curves.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// A 2D point in user space.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl core::ops::Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl core::ops::Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl core::ops::Mul<f64> for Point {
    type Output = Point;
    fn mul(self, rhs: f64) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

/// A 2D affine transform matrix in the form used by PostScript/PDF:
///
/// ```text
/// | a  b  0 |
/// | c  d  0 |
/// | e  f  1 |
/// ```
///
/// which maps `(x, y)` to `(a*x + c*y + e, b*x + d*y + f)`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub const fn identity() -> Self {
        Self::IDENTITY
    }

    /// Construct a transform from its six matrix components.
    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// Compose `self` after `other`: returns `self ∘ other`.
    pub fn concat(&self, other: &Transform) -> Transform {
        Transform {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// Apply the transform to a point.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }
}

/// A cubic Bézier segment with its control and end points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CubicBezier {
    pub start: Point,
    pub control1: Point,
    pub control2: Point,
    pub end: Point,
}

/// A single subpath: a sequence of line/curve segments that may be open or closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subpath {
    /// The starting point of the subpath.
    pub start: Point,
    /// Curve/line segments following the start point.
    pub segments: Vec<CubicBezier>,
    /// Whether the subpath is closed (last segment connects back to `start`).
    pub closed: bool,
}

impl Subpath {
    pub fn new(start: Point) -> Self {
        Self {
            start,
            segments: Vec::new(),
            closed: false,
        }
    }

    /// Append a cubic Bézier segment to this subpath.
    pub fn push_curve(&mut self, curve: CubicBezier) {
        self.segments.push(curve);
    }
}

/// A complete path: zero or more subpaths.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub subpaths: Vec<Subpath>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.is_empty()
    }
}
