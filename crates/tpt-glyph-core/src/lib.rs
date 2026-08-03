// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core
//
// Core rendering engine: graphics state, geometry primitives, canvas abstraction,
// and the per-page parallel rendering model. The `GraphicsState` is an immutable
// context passed down the rendering tree, eliminating the global-state hazards of
// the legacy C implementation.

//! # tpt-glyph-core
//!
//! Core rendering engine for TPT Glyph.
//!
//! TPT Glyph is a secure, sandboxed, multi-threaded PDF/PostScript rendering
//! engine. The defining architectural choice — inherited from the rendering
//! pipeline knowledge graph — is that the **graphics state is an immutable
//! context struct** (`GraphicsState`) passed down the rendering tree, rather
//! than a set of mutable global variables. This makes per-page concurrent
//! rendering across threads safe by construction.
//!
//! # Example
//!
//! Build a `RenderTree` for a filled rectangle and rasterize it with the
//! auto-selected backend:
//!
//! ```no_run
//! use tpt_glyph_core::backend::SelectedBackend;
//! use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath};
//! use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
//! use tpt_glyph_core::render::RenderTree;
//!
//! let mut tree = RenderTree::new(64, 64);
//! let state = GraphicsState::new().with_fill_color(RgbColor::new(0.9, 0.2, 0.1));
//! tree.fill(&state, square(Point::new(8.0, 8.0), 48.0));
//!
//! let backend = SelectedBackend::auto(None);
//! let canvas = backend.rasterize(&tree).expect("rasterize");
//! assert_eq!(canvas.width, 64);
//! # fn square(origin: Point, size: f64) -> Path {
//! #     Path { subpaths: vec![Subpath {
//! #         start: origin,
//! #         segments: vec![
//! #             hline(origin.x, origin.x + size, origin.y),
//! #             vline(origin.x + size, origin.y, origin.y + size),
//! #             hline(origin.x + size, origin.x, origin.y + size),
//! #             vline(origin.x, origin.y + size, origin.y),
//! #         ],
//! #         closed: true,
//! #     }] }
//! # }
//! # fn hline(x0: f64, x1: f64, y: f64) -> CubicBezier {
//! #     CubicBezier { start: Point::new(x0, y), control1: Point::new((x0 + x1) / 2.0, y),
//! #         control2: Point::new((x0 + x1) / 2.0, y), end: Point::new(x1, y) }
//! # }
//! # fn vline(x: f64, y0: f64, y1: f64) -> CubicBezier {
//! #     CubicBezier { start: Point::new(x, y0), control1: Point::new(x, (y0 + y1) / 2.0),
//! #         control2: Point::new(x, (y0 + y1) / 2.0), end: Point::new(x, y1) }
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#[macro_use]
extern crate alloc;

pub mod backend;
pub mod canvas;
pub mod document;
pub mod error;
pub mod geometry;
pub mod graphics_state;
pub mod math;
pub mod raster;
pub mod render;

#[cfg(feature = "raqote-backend")]
pub mod backends;

pub use error::{GlyphError, Result};
