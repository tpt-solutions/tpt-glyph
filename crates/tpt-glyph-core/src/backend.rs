// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / backend
//
// Backend abstraction over rasterizers. The pipeline is backend-agnostic: it
// produces a `RenderTree` and asks a `Backend` to rasterize it. This module
// owns runtime backend selection (GPU when available, CPU fallback) and the
// mapping from selection to a concrete `Rasterizer`.

use crate::canvas::Canvas;
use crate::error::Result;
use crate::raster::SoftwareRasterizer;
use crate::render::{DrawCommand, Rasterizer, RenderTree};

/// A named rasterization backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// Reference CPU scanline rasterizer (always available).
    #[default]
    Cpu,
    /// Accelerated CPU rasterizer backed by `raqote` (deterministic fallback).
    CpuRaqote,
    /// GPU rasterizer via `wgpu` (Phase 6 — currently falls back to CPU).
    Gpu,
}

impl Backend {
    /// Select the best available backend at runtime.
    ///
    /// The GPU path is not yet implemented, so auto-selection resolves to the
    /// CPU reference backend. The `raqote` backend is preferred over the
    /// reference backend when it is compiled in, because it is a more mature,
    /// antialiased CPU rasterizer. The wgpu detection hook is left in place for
    /// Phase 6.
    pub fn select(preferred: Option<Backend>) -> Backend {
        match preferred {
            Some(b) => b,
            None => {
                // TODO(Phase 6): probe for a wgpu adapter; if present return Gpu.
                if cfg!(feature = "raqote-backend") {
                    Backend::CpuRaqote
                } else {
                    Backend::Cpu
                }
            }
        }
    }
}

/// A runtime-resolved backend that knows how to rasterize a `RenderTree`.
pub struct SelectedBackend {
    kind: Backend,
    rasterizer: Box<dyn Rasterizer>,
}

impl SelectedBackend {
    /// Resolve `backend` into a concrete, usable backend.
    pub fn new(backend: Backend) -> Self {
        let rasterizer: Box<dyn Rasterizer> = match backend {
            Backend::Cpu => Box::new(SoftwareRasterizer::new()),
            Backend::CpuRaqote => {
                #[cfg(feature = "raqote-backend")]
                {
                    Box::new(crate::backends::raqote::RaqoteRasterizer::new())
                }
                #[cfg(not(feature = "raqote-backend"))]
                {
                    // Fall back to the reference backend if raqote was not compiled in.
                    Box::new(SoftwareRasterizer::new())
                }
            }
            Backend::Gpu => Box::new(SoftwareRasterizer::new()),
        };
        Self {
            kind: backend,
            rasterizer,
        }
    }

    /// Auto-select and resolve the best available backend.
    pub fn auto(preferred: Option<Backend>) -> Self {
        Self::new(Backend::select(preferred))
    }

    pub fn kind(&self) -> Backend {
        self.kind
    }

    pub fn rasterize(&self, tree: &RenderTree) -> Result<Canvas> {
        self.rasterizer.rasterize(tree)
    }
}

/// Convenience: rasterize a tree with the auto-selected backend.
pub fn rasterize_auto(tree: &RenderTree) -> Result<Canvas> {
    SelectedBackend::auto(None).rasterize(tree)
}

// Keep the export of `DrawCommand` meaningful for downstream tooling that builds
// trees against this module.
#[allow(dead_code)]
fn _assert_command(_c: &DrawCommand) {}
