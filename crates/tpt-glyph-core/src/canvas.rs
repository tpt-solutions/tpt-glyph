// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-core / canvas
//
// Backend-agnostic pixel buffer abstraction. A `Canvas` owns an RGBA buffer and
// its dimensions; rasterization backends (wgpu, raqote) draw into a buffer that
// conforms to this layout. The pipeline itself never depends on a concrete
// backend.

use crate::graphics_state::RgbColor;

/// RGBA pixel, 8 bits per channel, stored little-endian (R, G, B, A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    pub const TRANSPARENT: Pixel = Pixel {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Pixel = Pixel {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// An owned, backend-agnostic raster buffer in RGBA8 format.
///
/// Pixels are row-major, `width * height` in length, with the top-left pixel
/// at index `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Pixel>,
}

impl Canvas {
    /// Allocate a canvas of the given size, initialized to `background`.
    pub fn new(width: u32, height: u32, background: Pixel) -> Self {
        Self {
            width,
            height,
            pixels: vec![background; (width as usize) * (height as usize)],
        }
    }

    /// Number of pixels (`width * height`).
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Index of the pixel at `(x, y)`. Panics if out of bounds.
    pub fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    /// Blend a color onto a pixel using standard (`src-over`) alpha compositing.
    pub fn blend(&mut self, x: u32, y: u32, color: RgbColor, alpha: f64) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = self.index(x, y);
        let px = &mut self.pixels[i];
        let a = alpha.clamp(0.0, 1.0);
        let inv = 1.0 - a;
        let r = (color.r * 255.0).round() as u8;
        let g = (color.g * 255.0).round() as u8;
        let b = (color.b * 255.0).round() as u8;
        px.r = ((r as f64) * a + (px.r as f64) * inv).round() as u8;
        px.g = ((g as f64) * a + (px.g as f64) * inv).round() as u8;
        px.b = ((b as f64) * a + (px.b as f64) * inv).round() as u8;
        px.a = ((255.0) * a + (px.a as f64) * inv).round() as u8;
    }

    /// Convert the canvas into an `image::RgbaImage` for export.
    pub fn to_rgba_image(&self) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(self.width, self.height);
        for (i, px) in self.pixels.iter().enumerate() {
            let x = (i % self.width as usize) as u32;
            let y = (i / self.width as usize) as u32;
            img.put_pixel(x, y, image::Rgba([px.r, px.g, px.b, px.a]));
        }
        img
    }

    /// Write the canvas to a PNG file at `path`.
    pub fn save_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        self.to_rgba_image()
            .save(path)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}
