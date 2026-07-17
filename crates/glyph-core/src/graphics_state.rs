// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-core / graphics_state
//
// The immutable graphics state context. This struct is the heart of TPT Glyph's
// safety model: instead of Ghostscript's mutable global state, every operator
// receives the current `GraphicsState` and produces a new (derived) one. Because
// the state contains no shared mutable references, it can be freely copied into
// other threads for concurrent per-page rendering.

use crate::geometry::{Point, Transform};
use serde::{Deserialize, Serialize};

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
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            stroke_color: RgbColor::BLACK,
            fill_color: RgbColor::BLACK,
            line_width: 1.0,
            ctm: Transform::identity(),
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
