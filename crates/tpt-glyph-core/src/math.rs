// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / math
//
// Float-math shims for `no_std` builds. In `std` builds `f64` provides these
// methods inherently; without `std` they are supplied by `libm`. Importing
// `F64MathExt` is harmless in `std` builds because inherent methods take
// priority over trait methods.

/// `f64` math methods that are `std`-only, exposed as a trait so the same
/// call sites (`x.round()`, `x.sqrt()`, ...) compile in `no_std` builds.
pub trait F64MathExt {
    fn round(self) -> f64;
    fn sqrt(self) -> f64;
    fn ceil(self) -> f64;
    fn floor(self) -> f64;
    fn cos(self) -> f64;
    fn sin(self) -> f64;
}

#[cfg(not(feature = "std"))]
impl F64MathExt for f64 {
    fn round(self) -> f64 {
        libm::round(self)
    }

    fn sqrt(self) -> f64 {
        libm::sqrt(self)
    }

    fn ceil(self) -> f64 {
        libm::ceil(self)
    }

    fn floor(self) -> f64 {
        libm::floor(self)
    }

    fn cos(self) -> f64 {
        libm::cos(self)
    }

    fn sin(self) -> f64 {
        libm::sin(self)
    }
}
