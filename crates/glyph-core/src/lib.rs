// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-core
//
// Core rendering engine: graphics state, geometry primitives, canvas abstraction,
// and the per-page parallel rendering model. The `GraphicsState` is an immutable
// context passed down the rendering tree, eliminating the global-state hazards of
// the legacy C implementation.

//! # glyph-core
//!
//! Core rendering engine for TPT Glyph.
//!
//! TPT Glyph is a secure, sandboxed, multi-threaded PDF/PostScript rendering
//! engine. The defining architectural choice — inherited from the rendering
//! pipeline knowledge graph — is that the **graphics state is an immutable
//! context struct** (`GraphicsState`) passed down the rendering tree, rather
//! than a set of mutable global variables. This makes per-page concurrent
//! rendering across threads safe by construction.

pub mod canvas;
pub mod document;
pub mod error;
pub mod geometry;
pub mod graphics_state;

pub use error::{GlyphError, Result};
