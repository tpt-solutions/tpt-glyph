// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/constants
//
// Math layout constants, named after the OpenType MATH table's constant
// vocabulary (the standard reference for this data). Without parsing a real
// MATH table, every field here is a documented approximation derived from
// the current font's x-height and a default rule thickness, loosely modeled
// on traditional TeX math font (Computer Modern) proportions. The struct's
// shape is intentionally MATH-table-shaped so a future real table reader
// could populate the same fields without touching any call site.

use tpt_glyph_font::Font;

/// Layout constants for one font at one size, all expressed in the same
/// units as `font_size` (typically points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MathConstants {
    /// Default scale-down applied to Script-size material (OpenType MATH
    /// table default is 70%; real fonts may differ, we don't have one).
    pub script_scale_down: f64,
    /// Default scale-down applied to ScriptScript-size material (default 50%).
    pub script_script_scale_down: f64,

    /// Height of the mathematical axis above the baseline (the vertical
    /// center that fraction bars and relation symbols like `=` align to).
    pub axis_height: f64,
    /// Default thickness of a drawn rule (fraction bar, over/underline, ...).
    pub default_rule_thickness: f64,

    pub superscript_shift_up: f64,
    pub superscript_shift_up_cramped: f64,
    pub superscript_bottom_min: f64,
    pub subscript_shift_down: f64,
    pub subscript_top_max: f64,
    pub sub_superscript_gap_min: f64,

    pub fraction_numerator_shift_up: f64,
    pub fraction_numerator_display_style_shift_up: f64,
    pub fraction_denominator_shift_down: f64,
    pub fraction_denominator_display_style_shift_down: f64,
    pub fraction_numerator_gap_min: f64,
    pub fraction_num_display_style_gap_min: f64,
    pub fraction_denominator_gap_min: f64,
    pub fraction_denom_display_style_gap_min: f64,
    pub fraction_rule_thickness: f64,

    pub overbar_vertical_gap: f64,
    pub overbar_rule_thickness: f64,
    pub overbar_extra_ascender: f64,
    pub underbar_vertical_gap: f64,
    pub underbar_rule_thickness: f64,
    pub underbar_extra_descender: f64,

    pub radical_vertical_gap: f64,
    pub radical_display_style_vertical_gap: f64,
    pub radical_rule_thickness: f64,
    pub radical_extra_ascender: f64,

    pub accent_base_height: f64,
}

impl MathConstants {
    /// Derive constants for `font` rendered at `font_size` (same units as
    /// the returned fields, e.g. points).
    pub fn from_font(font: &Font, font_size: f64) -> Self {
        Self::from_x_height(x_height_of(font, font_size))
    }

    /// Derive constants directly from a known x-height, bypassing font
    /// lookup (useful for tests, or fonts without an `'x'` glyph).
    pub fn from_x_height(x_height: f64) -> Self {
        let rule = 0.1 * x_height;
        MathConstants {
            script_scale_down: 0.70,
            script_script_scale_down: 0.50,

            axis_height: 0.5 * x_height,
            default_rule_thickness: rule,

            superscript_shift_up: 1.0 * x_height,
            superscript_shift_up_cramped: 0.7 * x_height,
            superscript_bottom_min: 0.25 * x_height,
            subscript_shift_down: 0.6 * x_height,
            subscript_top_max: 0.6 * x_height,
            sub_superscript_gap_min: 4.0 * rule,

            fraction_numerator_shift_up: 1.3 * x_height,
            fraction_numerator_display_style_shift_up: 1.9 * x_height,
            fraction_denominator_shift_down: 0.9 * x_height,
            fraction_denominator_display_style_shift_down: 1.9 * x_height,
            fraction_numerator_gap_min: 1.0 * rule,
            fraction_num_display_style_gap_min: 3.0 * rule,
            fraction_denominator_gap_min: 1.0 * rule,
            fraction_denom_display_style_gap_min: 3.0 * rule,
            fraction_rule_thickness: rule,

            overbar_vertical_gap: 3.0 * rule,
            overbar_rule_thickness: rule,
            overbar_extra_ascender: rule,
            underbar_vertical_gap: 3.0 * rule,
            underbar_rule_thickness: rule,
            underbar_extra_descender: rule,

            radical_vertical_gap: 1.25 * rule,
            radical_display_style_vertical_gap: rule + 0.25 * x_height,
            radical_rule_thickness: rule,
            radical_extra_ascender: rule,

            accent_base_height: x_height,
        }
    }
}

/// The x-height of `font` at `font_size`, via `'x'`'s glyph bounding box.
/// Falls back to a 0.45em heuristic if the font has no `'x'` glyph (e.g. a
/// symbol-only font).
fn x_height_of(font: &Font, font_size: f64) -> f64 {
    const FALLBACK_RATIO: f32 = 0.45;
    let ratio = font
        .glyph_for_char('x')
        .and_then(|gid| font.glyph_metrics(gid))
        .map(|m| font.to_em_scale(m.bbox.height()))
        .unwrap_or(FALLBACK_RATIO);
    ratio as f64 * font_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_from_x_height_are_positive_and_ordered() {
        let k = MathConstants::from_x_height(4.3);
        assert!(k.axis_height > 0.0);
        assert!(k.default_rule_thickness > 0.0);
        assert!(k.superscript_shift_up > k.superscript_shift_up_cramped);
        assert!(k.fraction_numerator_display_style_shift_up > k.fraction_numerator_shift_up);
        assert!(
            k.fraction_denominator_display_style_shift_down > k.fraction_denominator_shift_down
        );
        assert!(k.fraction_num_display_style_gap_min > k.fraction_numerator_gap_min);
    }

    #[test]
    fn zero_x_height_yields_zero_constants_not_nan() {
        let k = MathConstants::from_x_height(0.0);
        assert_eq!(k.axis_height, 0.0);
        assert_eq!(k.default_rule_thickness, 0.0);
    }

    #[cfg(feature = "std")]
    fn sample_font() -> Font {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found");
        Font::from_bytes(&data).expect("valid font")
    }

    #[test]
    #[cfg(feature = "std")]
    fn from_font_derives_a_plausible_x_height() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 10.0);
        // x_height for a normal text font at 10pt should be a fraction of
        // the em, not near-zero and not larger than the font size itself.
        assert!(k.axis_height > 0.0);
        assert!(k.axis_height < 10.0);
    }
}
