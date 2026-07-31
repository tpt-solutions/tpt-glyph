// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / graphics_state
//
// The immutable graphics state context. This struct is the heart of TPT Glyph's
// safety model: instead of Ghostscript's mutable global state, every operator
// receives the current `GraphicsState` and produces a new (derived) one. Because
// the state contains no shared mutable references, it can be freely copied into
// other threads for concurrent per-page rendering.

use crate::geometry::{Point, Transform};
use serde::{Deserialize, Serialize};

/// Line cap style (corresponds to PostScript `setlinecap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    ProjectingSquare,
}

/// Line join style (corresponds to PostScript `setlinejoin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// 8-bit-per-channel RGB color. Channels are normalized to `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl RgbColor {
    pub const BLACK: RgbColor = RgbColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };
    pub const WHITE: RgbColor = RgbColor {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };

    pub const fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    /// Clamp all channels into `[0.0, 1.0]`.
    pub fn clamped(self) -> Self {
        let clamp = |v: f64| v.clamp(0.0, 1.0);
        Self {
            r: clamp(self.r),
            g: clamp(self.g),
            b: clamp(self.b),
        }
    }
}

/// An **immutable** snapshot of the drawing context.
///
/// Operators produce a new `GraphicsState` rather than mutating `self`. The
/// `Clone` derive is trivial and cheap (small, `Copy`-friendly fields) so the
/// state can be duplicated when entering a `gsave`/`grestore` scope or when
/// dispatched to another thread.
///
/// ```
/// use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
///
/// let a = GraphicsState::new().with_fill_color(RgbColor::new(1.0, 0.0, 0.0));
/// let b = a.with_fill_color(RgbColor::new(0.0, 1.0, 0.0));
/// // Deriving a new state leaves the original unchanged.
/// assert_eq!(a.fill_color, RgbColor::new(1.0, 0.0, 0.0));
/// assert_eq!(b.fill_color, RgbColor::new(0.0, 1.0, 0.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphicsState {
    /// Current stroke color.
    pub stroke_color: RgbColor,
    /// Current fill color.
    pub fill_color: RgbColor,
    /// Line width in user-space units.
    pub line_width: f64,
    /// CTM: current transformation matrix mapping user space to device space.
    pub ctm: Transform,
    /// Line cap style (PostScript `setlinecap`).
    pub line_cap: LineCap,
    /// Line join style (PostScript `setlinejoin`).
    pub line_join: LineJoin,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            stroke_color: RgbColor::BLACK,
            fill_color: RgbColor::BLACK,
            line_width: 1.0,
            ctm: Transform::identity(),
            line_cap: LineCap::default(),
            line_join: LineJoin::default(),
        }
    }
}

impl GraphicsState {
    /// Construct the default initial graphics state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the stroke color, returning a new state (does not mutate `self`).
    pub fn with_stroke_color(mut self, color: RgbColor) -> Self {
        self.stroke_color = color.clamped();
        self
    }

    /// Set the fill color, returning a new state.
    pub fn with_fill_color(mut self, color: RgbColor) -> Self {
        self.fill_color = color.clamped();
        self
    }

    /// Set the line width, returning a new state.
    pub fn with_line_width(mut self, width: f64) -> Self {
        self.line_width = width.max(0.0);
        self
    }

    /// Set the line cap style, returning a new state.
    pub fn with_line_cap(mut self, cap: LineCap) -> Self {
        self.line_cap = cap;
        self
    }

    /// Set the line join style, returning a new state.
    pub fn with_line_join(mut self, join: LineJoin) -> Self {
        self.line_join = join;
        self
    }

    /// Concatenate a user-space transform onto the CTM, returning a new state.
    ///
    /// The new transform is `ctm ∘ m`, matching PostScript `concat` semantics.
    pub fn concat_transform(mut self, m: &Transform) -> Self {
        self.ctm = self.ctm.concat(m);
        self
    }

    /// Transform a user-space point into device space using the current CTM.
    pub fn to_device(&self, p: Point) -> Point {
        self.ctm.apply(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_operators_do_not_mutate_self() {
        let state = GraphicsState::new();
        let original = state;

        // Each `with_*` returns a NEW state; the original must be unchanged.
        let s2 = state.with_stroke_color(RgbColor::new(1.0, 0.0, 0.0));
        let s3 = s2.with_line_width(5.0);
        let s4 = s3.concat_transform(&Transform::new(2.0, 0.0, 0.0, 2.0, 10.0, 10.0));

        assert_eq!(state, original, "mutating methods must not change self");
        assert_ne!(state, s2);
        assert_ne!(s2, s3);
        assert_ne!(s3, s4);
        assert_eq!(state.stroke_color, RgbColor::BLACK);
        assert_eq!(state.line_width, 1.0);
        assert_eq!(state.ctm, Transform::IDENTITY);
    }

    #[test]
    fn colors_are_clamped() {
        let s = GraphicsState::new().with_fill_color(RgbColor::new(-1.0, 2.0, 0.5));
        assert_eq!(s.fill_color, RgbColor::new(0.0, 1.0, 0.5));
    }

    #[test]
    fn concat_semantics_match_ps() {
        // PostScript `concat` composes the new matrix onto the CTM.
        let s =
            GraphicsState::new().concat_transform(&Transform::new(1.0, 0.0, 0.0, 1.0, 100.0, 50.0));
        let p = s.to_device(Point::new(10.0, 20.0));
        assert_eq!(p, Point::new(110.0, 70.0));
    }

    #[test]
    fn is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GraphicsState>();
    }
}
