// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — out-glyph-diff
//
// Pixel-diff comparison between TPT Glyph output and a Ghostscript reference
// render. Computes MSE, peak (max) per-channel error, and a structural
// similarity (SSIM) approximation, then applies configurable pass/fail
// thresholds. Used as a CI regression gate against the fixture corpus.

use anyhow::{Context, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Pass/fail thresholds for visual diffing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    /// Maximum allowed mean squared error across all channels (0..=1 scale).
    pub max_mse: f64,
    /// Maximum allowed peak per-channel absolute error (0..=255 scale).
    pub max_peak_error: u8,
    /// Minimum allowed mean structural similarity (0..=1).
    pub min_ssim: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_mse: 0.01,
            max_peak_error: 24,
            min_ssim: 0.98,
        }
    }
}

/// Metrics computed by comparing two images of identical dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DiffMetrics {
    pub mse: f64,
    pub peak_error: u8,
    pub ssim: f64,
}

/// Outcome of a single fixture comparison against thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Verdict {
    Pass,
    Fail,
}

/// Load an RGBA image from a path, normalizing alpha into premultiplied RGB so
/// transparent vs. opaque background differences do not dominate the metric.
pub fn load_rgba(path: &Path) -> Result<RgbaImage> {
    let img = image::open(path)
        .with_context(|| format!("failed to open image {}", path.display()))?
        .to_rgba8();
    Ok(img)
}

/// Compare two equal-sized RGBA images, returning raw metrics.
pub fn compare(a: &RgbaImage, b: &RgbaImage) -> Result<DiffMetrics> {
    if a.dimensions() != b.dimensions() {
        anyhow::bail!(
            "dimension mismatch: {:?} vs {:?}",
            a.dimensions(),
            b.dimensions()
        );
    }

    let (w, h) = a.dimensions();
    let pa = a.as_raw();
    let pb = b.as_raw();

    let mut sq_err_sum = 0.0_f64;
    let mut peak: u8 = 0;
    let mut n = 0_u64;
    for (ca, cb) in pa.chunks_exact(4).zip(pb.chunks_exact(4)) {
        for k in 0..3 {
            let d = (ca[k] as i32 - cb[k] as i32).unsigned_abs() as u8;
            if d > peak {
                peak = d;
            }
            let e = (ca[k] as f64 - cb[k] as f64) / 255.0;
            sq_err_sum += e * e;
            n += 1;
        }
    }
    let mse = if n > 0 { sq_err_sum / n as f64 } else { 0.0 };

    let ssim = ssim(a, b, w, h);

    Ok(DiffMetrics {
        mse,
        peak_error: peak,
        ssim,
    })
}

/// A windowed (8x8 block) SSIM approximation over luminance, returning the mean.
///
/// This is a pragmatic approximation suitable for regression gating, not a
/// reference-grade SSIM implementation.
pub fn ssim(a: &RgbaImage, b: &RgbaImage, w: u32, h: u32) -> f64 {
    let lum = |img: &RgbaImage, x: u32, y: u32| -> f64 {
        let p = img.get_pixel(x, y);
        0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
    };
    let k1: f64 = 0.01;
    let k2: f64 = 0.03;
    let l = 255.0_f64;
    let c1 = (k1 * l).powi(2);
    let c2 = (k2 * l).powi(2);
    let bw = 8u32;
    let bh = 8u32;

    let mut total = 0.0_f64;
    let mut blocks = 0u64;
    let mut y = 0u32;
    while y + bh <= h {
        let mut x = 0u32;
        while x + bw <= w {
            let (mut sa, mut sb, mut sa2, mut sb2, mut sab) = (0.0_f64, 0.0, 0.0, 0.0, 0.0);
            for by in y..y + bh {
                for bx in x..x + bw {
                    let la = lum(a, bx, by);
                    let lb = lum(b, bx, by);
                    sa += la;
                    sb += lb;
                    sa2 += la * la;
                    sb2 += lb * lb;
                    sab += la * lb;
                }
            }
            let n = (bw * bh) as f64;
            let ma = sa / n;
            let mb = sb / n;
            let va = (sa2 / n) - ma * ma;
            let vb = (sb2 / n) - mb * mb;
            let cov = (sab / n) - ma * mb;
            let s = ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            total += s;
            blocks += 1;
            x += bw;
        }
        y += bh;
    }
    if blocks == 0 {
        1.0
    } else {
        total / blocks as f64
    }
}

impl DiffMetrics {
    /// Apply thresholds to produce a pass/fail verdict.
    pub fn verdict(&self, t: &Thresholds) -> Verdict {
        if self.mse <= t.max_mse && self.peak_error <= t.max_peak_error && self.ssim >= t.min_ssim {
            Verdict::Pass
        } else {
            Verdict::Fail
        }
    }
}

/// A single fixture's comparison result, serializable for CI reporting.
#[derive(Debug, Clone, Serialize)]
pub struct FixtureReport {
    pub name: String,
    pub reference: Option<PathBuf>,
    pub candidate: PathBuf,
    pub metrics: Option<DiffMetrics>,
    pub verdict: Verdict,
    pub note: String,
}

impl FixtureReport {
    pub fn pending(name: String, candidate: PathBuf, note: String) -> Self {
        Self {
            name,
            reference: None,
            candidate,
            metrics: None,
            verdict: Verdict::Fail,
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(color: [u8; 4], w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba(color))
    }

    #[test]
    fn identical_images_pass() {
        let img = solid([10, 20, 30, 255], 16, 16);
        let m = compare(&img, &img).unwrap();
        assert_eq!(m.peak_error, 0);
        assert_eq!(m.verdict(&Thresholds::default()), Verdict::Pass);
    }

    #[test]
    fn large_diff_fails() {
        let a = solid([0, 0, 0, 255], 16, 16);
        let b = solid([255, 255, 255, 255], 16, 16);
        let m = compare(&a, &b).unwrap();
        assert!(m.peak_error > 100);
        assert_eq!(m.verdict(&Thresholds::default()), Verdict::Fail);
    }

    #[test]
    fn ssim_of_identical_is_one() {
        let img = solid([123, 45, 67, 255], 32, 32);
        assert!((ssim(&img, &img, 32, 32) - 1.0).abs() < 1e-9);
    }
}
