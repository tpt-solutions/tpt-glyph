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
use alloc::boxed::Box;

/// A named rasterization backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// Reference CPU scanline rasterizer (always available).
    #[default]
    Cpu,
    /// Accelerated CPU rasterizer backed by `raqote` (deterministic fallback).
    CpuRaqote,
    /// GPU rasterizer via `wgpu`, tessellating fills/strokes and rendering
    /// into a headless offscreen texture. Falls back to a CPU backend if no
    /// adapter is available at runtime (see `SelectedBackend::new`).
    Gpu,
}

impl Backend {
    /// Select the best available backend at runtime.
    ///
    /// A GPU adapter is preferred when the `wgpu-backend` feature is
    /// compiled in and an adapter is actually available at runtime (a real
    /// probe, not just a compile-time check — a build with the feature
    /// enabled still needs to run somewhere with a usable adapter). The
    /// `raqote` CPU backend is the next preference when compiled in, as a
    /// more mature, antialiased CPU rasterizer than the plain reference one.
    pub fn select(preferred: Option<Backend>) -> Backend {
        match preferred {
            Some(b) => b,
            None => {
                #[cfg(feature = "wgpu-backend")]
                if crate::backends::wgpu::WgpuRasterizer::adapter_available() {
                    return Backend::Gpu;
                }
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
            Backend::Gpu => {
                #[cfg(feature = "wgpu-backend")]
                {
                    match crate::backends::wgpu::WgpuRasterizer::new() {
                        Some(r) => Box::new(r),
                        // No adapter available at runtime — fall back rather
                        // than fail outright.
                        None => Box::new(SoftwareRasterizer::new()),
                    }
                }
                #[cfg(not(feature = "wgpu-backend"))]
                {
                    Box::new(SoftwareRasterizer::new())
                }
            }
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

    /// Borrow the resolved backend as a `&dyn Rasterizer`, for callers (e.g.
    /// `tpt-glyph-pdf::render_page`) that take the rasterizer as a parameter
    /// instead of owning a `SelectedBackend` themselves.
    pub fn as_rasterizer(&self) -> &dyn Rasterizer {
        self.rasterizer.as_ref()
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
