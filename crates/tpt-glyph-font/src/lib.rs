// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-font
//
// Font metric parsing and glyph outlining for TTF/OTF via ttf-parser. This
// crate is `no_std`-compatible (with `alloc`) so it can be used in WASM and
// embedded environments.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::vec::Vec;

/// A 2D point in font design space (font units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A rectangle in font design space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

impl Rect {
    pub const fn new(x_min: f32, y_min: f32, x_max: f32, y_max: f32) -> Self {
        Self {
            x_min,
            y_min,
            x_max,
            y_max,
        }
    }

    pub fn width(&self) -> f32 {
        self.x_max - self.x_min
    }

    pub fn height(&self) -> f32 {
        self.y_max - self.y_min
    }
}

/// A 16-bit glyph identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u16);

/// Metrics for a single glyph, in font units.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance_width: f32,
    pub bbox: Rect,
}

/// A segment in a glyph outline.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    LineTo(Point),
    QuadTo {
        control: Point,
        to: Point,
    },
    CurveTo {
        control1: Point,
        control2: Point,
        to: Point,
    },
}

/// A closed contour in a glyph outline.
#[derive(Debug, Clone)]
pub struct Contour {
    pub start: Point,
    pub segments: Vec<Segment>,
}

impl Contour {
    pub fn new(start: Point) -> Self {
        Self {
            start,
            segments: Vec::new(),
        }
    }
}

/// The vector outline of a glyph.
#[derive(Debug, Clone)]
pub struct GlyphOutline {
    pub contours: Vec<Contour>,
}

/// A horizontal kerning pair.
#[derive(Debug, Clone, Copy)]
pub struct KerningPair {
    pub left: GlyphId,
    pub right: GlyphId,
    pub advance: f32,
}

/// A parsed font.
///
/// Stores the raw font bytes so that `Face` can be re-parsed on demand
/// (ttf-parser's `Face` borrows the data and does not own it).
#[derive(Debug, Clone)]
pub struct Font {
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
    glyph_count: u16,
    raw: Vec<u8>,
}

impl Font {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let face = ttf_parser::Face::parse(data, 0).ok()?;
        Some(Self {
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            line_gap: face.line_gap(),
            glyph_count: face.number_of_glyphs(),
            raw: data.to_vec(),
        })
    }

    pub fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn ascender(&self) -> i16 {
        self.ascender
    }

    pub fn descender(&self) -> i16 {
        self.descender
    }

    pub fn line_gap(&self) -> i16 {
        self.line_gap
    }

    pub fn to_em_scale(&self, value: f32) -> f32 {
        value / self.units_per_em as f32
    }

    pub fn glyph_for_char(&self, c: char) -> Option<GlyphId> {
        let face = ttf_parser::Face::parse(&self.raw, 0).ok()?;
        face.glyph_index(c).map(|g| GlyphId(g.0))
    }

    pub fn glyph_metrics(&self, gid: GlyphId) -> Option<GlyphMetrics> {
        let face = ttf_parser::Face::parse(&self.raw, 0).ok()?;
        let g = ttf_parser::GlyphId(gid.0);
        let advance = face.glyph_hor_advance(g)?;
        let bbox = face.glyph_bounding_box(g)?;
        Some(GlyphMetrics {
            advance_width: advance as f32,
            bbox: Rect::new(
                bbox.x_min as f32,
                bbox.y_min as f32,
                bbox.x_max as f32,
                bbox.y_max as f32,
            ),
        })
    }

    pub fn glyph_advance(&self, gid: GlyphId) -> Option<f32> {
        let face = ttf_parser::Face::parse(&self.raw, 0).ok()?;
        face.glyph_hor_advance(ttf_parser::GlyphId(gid.0))
            .map(|a| a as f32)
    }

    pub fn glyph_outline(&self, gid: GlyphId) -> Option<GlyphOutline> {
        let face = ttf_parser::Face::parse(&self.raw, 0).ok()?;
        let mut builder = OutlineBuilder {
            contours: Vec::new(),
            current_contour: None,
        };
        face.outline_glyph(ttf_parser::GlyphId(gid.0), &mut builder)?;
        if let Some(c) = builder.current_contour {
            builder.contours.push(c);
        }
        if builder.contours.is_empty() {
            return None;
        }
        Some(GlyphOutline {
            contours: builder.contours,
        })
    }

    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> f32 {
        let face = match ttf_parser::Face::parse(&self.raw, 0) {
            Ok(f) => f,
            Err(_) => return 0.0,
        };
        let left_gid = ttf_parser::GlyphId(left.0);
        let right_gid = ttf_parser::GlyphId(right.0);
        if let Some(kern) = face.tables().kern {
            for subtable in kern.subtables {
                if let Some(value) = subtable.glyphs_kerning(left_gid, right_gid) {
                    return value as f32;
                }
            }
        }
        0.0
    }

    pub fn all_kerning_pairs(&self) -> Vec<KerningPair> {
        let face = match ttf_parser::Face::parse(&self.raw, 0) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut pairs = Vec::new();
        if let Some(kern) = face.tables().kern {
            for subtable in kern.subtables {
                if let ttf_parser::kern::Format::Format0(sub0) = subtable.format {
                    for pair in sub0.pairs {
                        pairs.push(KerningPair {
                            left: GlyphId(pair.left().0),
                            right: GlyphId(pair.right().0),
                            advance: pair.value as f32,
                        });
                    }
                }
            }
        }
        pairs
    }

    pub fn line_height(&self) -> i16 {
        self.ascender - self.descender + self.line_gap
    }
}

struct OutlineBuilder {
    contours: Vec<Contour>,
    current_contour: Option<Contour>,
}

impl ttf_parser::OutlineBuilder for OutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if let Some(c) = self.current_contour.take() {
            self.contours.push(c);
        }
        self.current_contour = Some(Contour::new(Point::new(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(ref mut c) = self.current_contour {
            c.segments.push(Segment::LineTo(Point::new(x, y)));
        }
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if let Some(ref mut c) = self.current_contour {
            c.segments.push(Segment::QuadTo {
                control: Point::new(cx, cy),
                to: Point::new(x, y),
            });
        }
    }

    fn curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        if let Some(ref mut c) = self.current_contour {
            c.segments.push(Segment::CurveTo {
                control1: Point::new(cx1, cy1),
                control2: Point::new(cx2, cy2),
                to: Point::new(x, y),
            });
        }
    }

    fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_font() -> Font {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found; set GLYPH_FONT_TEST_PATH");
        Font::from_bytes(&data).expect("valid font")
    }

    #[test]
    fn parses_units_per_em() {
        let font = sample_font();
        assert!(font.units_per_em() >= 16);
        assert!(font.units_per_em() <= 16384);
    }

    #[test]
    fn glyph_count_is_positive() {
        let font = sample_font();
        assert!(font.glyph_count() > 0);
    }

    #[test]
    fn glyph_for_char_maps_known() {
        let font = sample_font();
        assert!(font.glyph_for_char('A').is_some());
    }

    #[test]
    fn glyph_advance_returns_some_for_known_glyph() {
        let font = sample_font();
        let gid = font.glyph_for_char('A').unwrap();
        let adv = font.glyph_advance(gid);
        assert!(adv.is_some());
        assert!(adv.unwrap() > 0.0);
    }

    #[test]
    fn glyph_metrics_returns_bbox() {
        let font = sample_font();
        let gid = font.glyph_for_char('A').unwrap();
        let metrics = font.glyph_metrics(gid);
        assert!(metrics.is_some());
        let m = metrics.unwrap();
        assert!(m.bbox.width() > 0.0);
    }

    #[test]
    fn glyph_outline_returns_contours_for_visible_glyph() {
        let font = sample_font();
        let gid = font.glyph_for_char('A').unwrap();
        let outline = font.glyph_outline(gid);
        assert!(outline.is_some());
        let o = outline.unwrap();
        assert!(!o.contours.is_empty());
    }

    #[test]
    fn space_glyph_has_no_outline_or_empty_one() {
        let font = sample_font();
        let space = font.glyph_for_char(' ').unwrap_or(GlyphId(1));
        let outline = font.glyph_outline(space);
        if let Some(o) = outline {
            assert!(o.contours.is_empty());
        }
    }

    #[test]
    fn kerning_returns_finite_value() {
        let font = sample_font();
        if let Some(a) = font.glyph_for_char('A') {
            let k = font.kerning(a, a);
            assert!(k.is_finite());
        }
    }

    #[test]
    fn ascender_is_positive_descender_is_negative_or_zero() {
        let font = sample_font();
        assert!(font.ascender() > 0);
        assert!(font.descender() <= 0);
    }

    #[test]
    fn to_em_scale_normalizes() {
        let font = sample_font();
        let scaled = font.to_em_scale(font.units_per_em() as f32);
        assert!((scaled - 1.0).abs() < 1e-6);
    }

    #[test]
    fn supports_unicode_glyphs() {
        let font = sample_font();
        // Try common chars
        for ch in ['A', 'z', '0', '.'] {
            assert!(
                font.glyph_for_char(ch).is_some(),
                "char '{ch}' should have a glyph"
            );
        }
    }
}
