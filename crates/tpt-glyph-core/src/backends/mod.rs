// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / backends
//
// Accelerated rasterization backends. Each backend implements the `Rasterizer`
// trait from `crate::render`. They are gated behind cargo features so the
// always-available reference `SoftwareRasterizer` stays the default build.

#[cfg(feature = "raqote-backend")]
pub mod raqote;
