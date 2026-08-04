// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-measure / text
//
// Text metrics (advance widths, ascent/descent) for a PDF font resource.
// When the font is embedded (`FontRef::embedded_font`), real glyph advances
// and font-derived ascent/descent come from `tpt-glyph-font`. Otherwise this
// falls back to the font resource's own `/Widths` array (present for almost
// every simple font, embedded or not) and typical Latin-text ascent/descent
// ratios, since PDF doesn't require ascent/descent for non-embedded fonts.

use tpt_glyph_font::Font;
use tpt_glyph_pdf_ir::FontRef;

/// A font's vertical metrics, scaled to a point size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
}

impl FontMetrics {
    /// Derive ascent/descent from an embedded font program, scaled to `size`.
    pub fn from_font(font: &Font, size: f64) -> Self {
        let upm = font.units_per_em().max(1) as f64;
        Self {
            ascent: font.ascender() as f64 / upm * size,
            descent: font.descender() as f64 / upm * size,
        }
    }
}

/// The measured extent of a run of text at a given point size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    /// Total horizontal advance, in the same units as `size`.
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
}

/// Typical Latin-text metrics (as fractions of the point size), used only as
/// a last resort when neither an embedded font program nor a `/Widths`
/// entry is available.
const FALLBACK_ASCENT_RATIO: f64 = 0.718;
const FALLBACK_DESCENT_RATIO: f64 = -0.207;
/// PDF glyph space is 1000 units/em; this is a plausible average glyph
/// width for a code missing from `/Widths` (`/MissingWidth` defaults to 0
/// per spec, but a silent zero-width advance is rarely what a caller wants
/// from a *measurement* tool, so this crate uses a non-zero average instead).
const FALLBACK_GLYPH_WIDTH_1000: f64 = 500.0;

/// Measure `text` as shown with `font` at `size` (PDF text-space units, i.e.
/// point size before any `Tz`/text-matrix scaling).
///
/// `text` is interpreted one byte per character code (correct for simple
/// Type1/TrueType fonts using a single-byte encoding — the common case);
/// Type0/CID composite fonts are not resolved to multi-byte codes here.
pub fn measure_text(font: &FontRef, text: &str, size: f64) -> TextMetrics {
    if let Some(embedded) = font.embedded_font.as_deref() {
        if let Some(parsed) = Font::from_bytes(embedded) {
            let upm = parsed.units_per_em().max(1) as f64;
            let advance: f64 = text
                .chars()
                .map(|c| {
                    parsed
                        .glyph_for_char(c)
                        .and_then(|g| parsed.glyph_advance(g))
                        .map(|a| a as f64 / upm * size)
                        .unwrap_or(size * FALLBACK_GLYPH_WIDTH_1000 / 1000.0)
                })
                .sum();
            let metrics = FontMetrics::from_font(&parsed, size);
            return TextMetrics {
                advance,
                ascent: metrics.ascent,
                descent: metrics.descent,
            };
        }
    }

    let advance: f64 = text
        .bytes()
        .map(|b| width_for_code(font, b as u32) / 1000.0 * size)
        .sum();

    TextMetrics {
        advance,
        ascent: size * FALLBACK_ASCENT_RATIO,
        descent: size * FALLBACK_DESCENT_RATIO,
    }
}

/// Look up a single character code's width (in PDF glyph-space/1000 units)
/// from the font resource's `/Widths` array, falling back to a generic
/// average glyph width for codes outside `[first_char, last_char]`.
fn width_for_code(font: &FontRef, code: u32) -> f64 {
    if code >= font.first_char {
        let idx = (code - font.first_char) as usize;
        if let Some(&w) = font.widths.get(idx) {
            return w;
        }
    }
    FALLBACK_GLYPH_WIDTH_1000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font_with_widths(first_char: u32, widths: Vec<f64>) -> FontRef {
        FontRef {
            subtype: "TrueType".into(),
            base_font: "Test".into(),
            first_char,
            last_char: first_char + (widths.len() as u32).saturating_sub(1),
            widths,
            descriptor: None,
            to_unicode: None,
            encoding: None,
            embedded_font: None,
        }
    }

    #[test]
    fn advance_sums_widths_array_entries() {
        // 'A' = 65, 'B' = 66, 'C' = 67.
        let font = font_with_widths(65, vec![600.0, 700.0, 800.0]);
        let m = measure_text(&font, "ABC", 10.0);
        // (600 + 700 + 800) / 1000 * 10 = 21.0
        assert!((m.advance - 21.0).abs() < 1e-9, "advance was {}", m.advance);
    }

    #[test]
    fn missing_width_entry_falls_back_to_average() {
        let font = font_with_widths(65, vec![600.0]); // only 'A' has a width
        let m = measure_text(&font, "AZ", 10.0);
        // 'A': 600/1000*10 = 6.0; 'Z' out of range: 500/1000*10 = 5.0
        assert!((m.advance - 11.0).abs() < 1e-9, "advance was {}", m.advance);
    }

    #[test]
    fn fallback_ascent_descent_are_reasonable_fractions_of_size() {
        let font = font_with_widths(0, vec![]);
        let m = measure_text(&font, "", 12.0);
        assert!(m.ascent > 0.0 && m.ascent < 12.0);
        assert!(m.descent < 0.0 && m.descent > -12.0);
    }
}
