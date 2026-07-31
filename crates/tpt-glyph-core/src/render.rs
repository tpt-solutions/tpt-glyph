// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / render
//
// The rendering tree / traversal model. Operators produce a stream of
// `DrawCommand`s against an immutable `GraphicsState`. The pipeline turns those
// commands into pixels through a backend-agnostic `Rasterizer` trait. This
// indirection is what keeps tpt-glyph-core free of any concrete GPU/CPU backend
// dependency (those live in Phase 6).

use crate::canvas::Canvas;
use crate::error::Result;
use crate::geometry::{Path, Point, Transform};
use crate::graphics_state::{GraphicsState, RgbColor};
use alloc::vec::Vec;

/// A single atomic drawing operation emitted by the operator pipeline.
///
/// All commands are pure with respect to the immutable `GraphicsState`: they
/// carry the exact state (color, transform, line width) needed to rasterize,
/// so no global context is required at draw time.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Stroke the given path using `state.stroke_color` and `state.line_width`,
    /// after transforming user space via `state.ctm`.
    Stroke {
        path: Path,
        color: RgbColor,
        line_width: f64,
        ctm: Transform,
    },
    /// Fill the given path using `state.fill_color`, after `state.ctm`.
    Fill {
        path: Path,
        color: RgbColor,
        ctm: Transform,
    },
    /// Fill the given path using the even-odd rule (PostScript `eofill`).
    FillEvenOdd {
        path: Path,
        color: RgbColor,
        ctm: Transform,
    },
    /// Set the clip region (intersection) — applied by backends during traversal.
    Clip { path: Path, ctm: Transform },
}

impl DrawCommand {
    /// The graphics state fragment this command was produced with.
    pub fn ctm(&self) -> &Transform {
        match self {
            DrawCommand::Stroke { ctm, .. }
            | DrawCommand::Fill { ctm, .. }
            | DrawCommand::FillEvenOdd { ctm, .. }
            | DrawCommand::Clip { ctm, .. } => ctm,
        }
    }
}

/// A rendered page is an ordered list of draw commands plus the device size.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderTree {
    pub width: u32,
    pub height: u32,
    pub commands: Vec<DrawCommand>,
}

impl RenderTree {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
        }
    }

    /// Append a stroke command derived from the current immutable state.
    pub fn stroke(&mut self, state: &GraphicsState, path: Path) {
        self.commands.push(DrawCommand::Stroke {
            path,
            color: state.stroke_color,
            line_width: state.line_width,
            ctm: state.ctm,
        });
    }

    /// Append a fill command derived from the current immutable state.
    pub fn fill(&mut self, state: &GraphicsState, path: Path) {
        self.commands.push(DrawCommand::Fill {
            path,
            color: state.fill_color,
            ctm: state.ctm,
        });
    }

    /// Append an even-odd fill command derived from the current immutable state.
    pub fn fill_even_odd(&mut self, state: &GraphicsState, path: Path) {
        self.commands.push(DrawCommand::FillEvenOdd {
            path,
            color: state.fill_color,
            ctm: state.ctm,
        });
    }

    /// Append a clip command derived from the current immutable state.
    pub fn clip(&mut self, state: &GraphicsState, path: Path) {
        self.commands.push(DrawCommand::Clip {
            path,
            ctm: state.ctm,
        });
    }
}

/// Backend-agnostic rasterizer: turns a `RenderTree` into a `Canvas`.
///
/// Implementors (wgpu in Phase 6, raqote in Phase 6) must not require any global
/// state, preserving thread-safe concurrent page rendering.
pub trait Rasterizer {
    fn rasterize(&self, tree: &RenderTree) -> Result<Canvas>;
}

/// A trivial reference rasterizer that fills the canvas background white and
/// stamps each command's bounding box with its color. This exists so the
/// pipeline and tests have a working backend before the GPU/CPU backends land,
/// and so Phase 1 harness fixtures can be produced end-to-end.
pub struct DebugRasterizer;

impl Rasterizer for DebugRasterizer {
    fn rasterize(&self, tree: &RenderTree) -> Result<Canvas> {
        let mut canvas = Canvas::new(tree.width, tree.height, Canvas::pixel_white());

        for cmd in &tree.commands {
            let color = match cmd {
                DrawCommand::Stroke { color, .. } => *color,
                DrawCommand::Fill { color, .. } => *color,
                DrawCommand::FillEvenOdd { color, .. } => *color,
                DrawCommand::Clip { .. } => continue,
            };
            // Stamp a 4x4 dot at each path start point (post-transform) so output
            // is visibly non-empty and deterministic for snapshot tests.
            let ctm = *cmd.ctm();
            for sub in path_points(cmd) {
                let p: Point = ctm.apply(sub);
                let (x, y) = (p.x as i32, p.y as i32);
                for dy in 0..4 {
                    for dx in 0..4 {
                        canvas.blend(
                            (x + dx).clamp(0, tree.width as i32 - 1) as u32,
                            (y + dy).clamp(0, tree.height as i32 - 1) as u32,
                            color,
                            1.0,
                        );
                    }
                }
            }
        }
        Ok(canvas)
    }
}

fn path_points(cmd: &DrawCommand) -> Vec<Point> {
    let path = match cmd {
        DrawCommand::Stroke { path, .. } => path,
        DrawCommand::Fill { path, .. } => path,
        DrawCommand::FillEvenOdd { path, .. } => path,
        DrawCommand::Clip { .. } => return Vec::new(),
    };
    path.subpaths.iter().map(|s| s.start).collect()
}

impl Canvas {
    pub fn pixel_white() -> crate::canvas::Pixel {
        crate::canvas::Pixel::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Subpath;

    #[test]
    fn stroke_captures_state_at_emit_time() {
        let mut tree = RenderTree::new(100, 100);
        let mut state = GraphicsState::new();
        state = state.with_stroke_color(RgbColor::new(0.0, 0.0, 1.0));

        let path = Path {
            subpaths: vec![Subpath::new(Point::new(0.0, 0.0))],
        };
        tree.stroke(&state, path.clone());

        // Changing state afterward must NOT affect the already-emitted command.
        state = state.with_stroke_color(RgbColor::new(1.0, 0.0, 0.0));
        tree.stroke(&state, path);

        match (&tree.commands[0], &tree.commands[1]) {
            (DrawCommand::Stroke { color: c0, .. }, DrawCommand::Stroke { color: c1, .. }) => {
                assert_eq!(*c0, RgbColor::new(0.0, 0.0, 1.0));
                assert_eq!(*c1, RgbColor::new(1.0, 0.0, 0.0));
            }
            _ => panic!("expected stroke commands"),
        }
    }

    #[test]
    fn debug_rasterizer_produces_non_empty_canvas() {
        let mut tree = RenderTree::new(64, 64);
        let state = GraphicsState::new().with_fill_color(RgbColor::new(1.0, 0.0, 0.0));
        tree.fill(
            &state,
            Path {
                subpaths: vec![Subpath::new(Point::new(10.0, 10.0))],
            },
        );
        let canvas = DebugRasterizer.rasterize(&tree).unwrap();
        assert_eq!(canvas.len(), 64 * 64);
        assert!(canvas.pixels.iter().any(|p| p.r > 0));
    }
}
