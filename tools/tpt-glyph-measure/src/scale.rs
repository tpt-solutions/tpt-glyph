// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-measure / scale
//
// Drawing-scale parsing and application. A `ScaleSpec` reduces to a single
// dimensionless factor — real-world length per drawn length, in the *same*
// unit — because that's all a scale ratio ever is: `1:100` means "100 real
// units per 1 drawn unit" regardless of which unit you measure in, and an
// architectural spec like `1/4in=1ft` is just that same ratio expressed via
// two concrete lengths instead of numerator/denominator.

use std::collections::HashMap;
use std::fmt;

/// A unit of physical length, for both scale-spec parsing and CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Mm,
    Cm,
    M,
    In,
    Ft,
}

impl LengthUnit {
    /// Parse a unit suffix. Longer suffixes are matched first so `"mm"`/
    /// `"cm"` aren't swallowed by the single-character `"m"`.
    fn parse_suffix(s: &str) -> Option<(Self, &str)> {
        const UNITS: &[(&str, LengthUnit)] = &[
            ("mm", LengthUnit::Mm),
            ("cm", LengthUnit::Cm),
            ("ft", LengthUnit::Ft),
            ("in", LengthUnit::In),
            ("m", LengthUnit::M),
        ];
        for (suffix, unit) in UNITS {
            if let Some(rest) = s.strip_suffix(suffix) {
                return Some((*unit, rest));
            }
        }
        None
    }

    fn mm_per_unit(self) -> f64 {
        match self {
            LengthUnit::Mm => 1.0,
            LengthUnit::Cm => 10.0,
            LengthUnit::M => 1000.0,
            LengthUnit::In => 25.4,
            LengthUnit::Ft => 304.8,
        }
    }

    pub fn from_mm(self, mm: f64) -> f64 {
        mm / self.mm_per_unit()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match Self::parse_suffix(s) {
            Some((unit, "")) => Some(unit),
            _ => None,
        }
    }
}

impl fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LengthUnit::Mm => "mm",
            LengthUnit::Cm => "cm",
            LengthUnit::M => "m",
            LengthUnit::In => "in",
            LengthUnit::Ft => "ft",
        })
    }
}

/// One PDF unit is exactly 1/72 inch by definition (PDF spec, independent of
/// the actual print/media size), so this conversion is always exact.
const MM_PER_PDF_UNIT: f64 = 25.4 / 72.0;

/// A parsed drawing scale, as real-world millimeters per drawn millimeter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleSpec {
    factor: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScaleParseError(pub String);

impl fmt::Display for ScaleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid scale spec: {}", self.0)
    }
}

impl std::error::Error for ScaleParseError {}

impl ScaleSpec {
    /// A 1:1 scale (drawn length == real-world length).
    pub const IDENTITY: ScaleSpec = ScaleSpec { factor: 1.0 };

    /// Parse a scale specification in one of two forms:
    ///
    /// - A ratio, `A:B` (e.g. `"1:100"`, `"1:50"`) — `B` real-world units
    ///   per `A` drawn units, any consistent unit.
    /// - An architectural/engineering equivalence, `<drawn>=<real>`, where
    ///   each side is a value (decimal, or a simple `a/b` fraction) followed
    ///   by a unit suffix (`mm`, `cm`, `m`, `in`, `ft`) — e.g.
    ///   `"1/4in=1ft"` (the standard 1:48 architectural scale) or
    ///   `"5mm=1m"`.
    pub fn parse(s: &str) -> Result<Self, ScaleParseError> {
        let s = s.trim();
        if let Some((drawn, real)) = s.split_once('=') {
            let drawn_mm = parse_length_mm(drawn.trim())?;
            let real_mm = parse_length_mm(real.trim())?;
            if drawn_mm <= 0.0 {
                return Err(ScaleParseError(format!(
                    "drawn length must be positive: {s}"
                )));
            }
            Ok(ScaleSpec {
                factor: real_mm / drawn_mm,
            })
        } else if let Some((a, b)) = s.split_once(':') {
            let a: f64 = a
                .trim()
                .parse()
                .map_err(|_| ScaleParseError(s.to_string()))?;
            let b: f64 = b
                .trim()
                .parse()
                .map_err(|_| ScaleParseError(s.to_string()))?;
            if a <= 0.0 {
                return Err(ScaleParseError(format!(
                    "ratio's drawn side must be positive: {s}"
                )));
            }
            Ok(ScaleSpec { factor: b / a })
        } else {
            Err(ScaleParseError(s.to_string()))
        }
    }

    /// Convert a length measured in PDF units (points, 1/72in) into
    /// real-world millimeters under this scale.
    pub fn real_world_mm(&self, pdf_units: f64) -> f64 {
        pdf_units * MM_PER_PDF_UNIT * self.factor
    }
}

/// Parse `"<value><unit>"`, where value is a decimal or an `a/b` fraction.
fn parse_length_mm(s: &str) -> Result<f64, ScaleParseError> {
    let (unit, value_str) = LengthUnit::parse_suffix(s)
        .ok_or_else(|| ScaleParseError(format!("missing/unknown unit in {s:?}")))?;
    let value = if let Some((num, den)) = value_str.split_once('/') {
        let num: f64 = num
            .trim()
            .parse()
            .map_err(|_| ScaleParseError(s.to_string()))?;
        let den: f64 = den
            .trim()
            .parse()
            .map_err(|_| ScaleParseError(s.to_string()))?;
        if den == 0.0 {
            return Err(ScaleParseError(format!("division by zero in {s:?}")));
        }
        num / den
    } else {
        value_str
            .trim()
            .parse()
            .map_err(|_| ScaleParseError(s.to_string()))?
    };
    Ok(value * unit.mm_per_unit())
}

/// A per-document scale table: an optional default scale plus 1-based
/// per-page overrides, so a single document can mix scales across pages
/// (e.g. a detail sheet at `1:20` alongside site plans at `1:500`).
#[derive(Debug, Clone, Default)]
pub struct ScaleTable {
    default: Option<ScaleSpec>,
    per_page: HashMap<usize, ScaleSpec>,
}

impl ScaleTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default(mut self, spec: ScaleSpec) -> Self {
        self.default = Some(spec);
        self
    }

    pub fn with_page(mut self, page: usize, spec: ScaleSpec) -> Self {
        self.per_page.insert(page, spec);
        self
    }

    /// The scale that applies to 1-based page `page`: its override if one
    /// was set, else the table's default, else [`ScaleSpec::IDENTITY`].
    pub fn scale_for(&self, page: usize) -> ScaleSpec {
        self.per_page
            .get(&page)
            .copied()
            .or(self.default)
            .unwrap_or(ScaleSpec::IDENTITY)
    }

    /// Parse a JSON config: `{"default": "1:100", "pages": {"1": "1/4in=1ft", "3": "1:50"}}`.
    /// Both `default` and `pages` are optional.
    pub fn from_json(json: &str) -> Result<Self, ScaleParseError> {
        #[derive(serde::Deserialize)]
        struct Raw {
            default: Option<String>,
            #[serde(default)]
            pages: HashMap<String, String>,
        }
        let raw: Raw = serde_json::from_str(json).map_err(|e| ScaleParseError(e.to_string()))?;
        let mut table = ScaleTable::new();
        if let Some(d) = raw.default {
            table = table.with_default(ScaleSpec::parse(&d)?);
        }
        for (page, spec) in raw.pages {
            let page: usize = page
                .parse()
                .map_err(|_| ScaleParseError(format!("bad page number: {page}")))?;
            table = table.with_page(page, ScaleSpec::parse(&spec)?);
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_scale_converts_pdf_units_to_mm() {
        // 1:100 with a 72-PDF-unit (1 inch = 25.4mm) drawn length -> 2540mm real.
        let scale = ScaleSpec::parse("1:100").unwrap();
        let mm = scale.real_world_mm(72.0);
        assert!((mm - 2540.0).abs() < 1e-6, "got {mm}");
    }

    #[test]
    fn architectural_quarter_inch_scale_is_1_to_48() {
        // 1/4in = 1ft is the standard 1:48 architectural scale.
        let scale = ScaleSpec::parse("1/4in=1ft").unwrap();
        let direct = ScaleSpec::parse("1:48").unwrap();
        assert!((scale.factor - direct.factor).abs() < 1e-9);
    }

    #[test]
    fn metric_architectural_scale() {
        // 5mm drawn = 1m real -> factor 200 (a common 1:200 site-plan scale).
        let scale = ScaleSpec::parse("5mm=1m").unwrap();
        assert!((scale.factor - 200.0).abs() < 1e-9);
    }

    #[test]
    fn output_unit_conversion_round_trips() {
        let scale = ScaleSpec::parse("1:100").unwrap();
        let mm = scale.real_world_mm(72.0); // 2540mm
        let m = LengthUnit::M.from_mm(mm);
        assert!((m - 2.54).abs() < 1e-9, "got {m}");
    }

    #[test]
    fn scale_table_mixes_per_page_scales_with_a_default() {
        let table = ScaleTable::new()
            .with_default(ScaleSpec::parse("1:100").unwrap())
            .with_page(2, ScaleSpec::parse("1:50").unwrap());
        assert_eq!(
            table.scale_for(1).real_world_mm(1.0),
            ScaleSpec::parse("1:100").unwrap().real_world_mm(1.0)
        );
        assert_eq!(
            table.scale_for(2).real_world_mm(1.0),
            ScaleSpec::parse("1:50").unwrap().real_world_mm(1.0)
        );
        assert_eq!(
            table.scale_for(3).real_world_mm(1.0),
            ScaleSpec::parse("1:100").unwrap().real_world_mm(1.0)
        );
    }

    #[test]
    fn scale_table_defaults_to_identity_when_unset() {
        let table = ScaleTable::new();
        assert_eq!(
            table.scale_for(1).real_world_mm(10.0),
            ScaleSpec::IDENTITY.real_world_mm(10.0)
        );
    }

    #[test]
    fn from_json_parses_default_and_per_page_scales() {
        let json = r#"{"default": "1:100", "pages": {"1": "1/4in=1ft", "3": "1:50"}}"#;
        let table = ScaleTable::from_json(json).unwrap();
        assert!(
            (table.scale_for(1).factor - ScaleSpec::parse("1/4in=1ft").unwrap().factor).abs()
                < 1e-9
        );
        assert!((table.scale_for(3).factor - 50.0).abs() < 1e-9);
        assert!((table.scale_for(2).factor - 100.0).abs() < 1e-9); // falls back to default
    }

    #[test]
    fn rejects_garbage_input() {
        assert!(ScaleSpec::parse("nonsense").is_err());
        assert!(ScaleSpec::parse("1:0").is_err() || ScaleSpec::parse("1:0").unwrap().factor == 0.0);
        assert!(ScaleSpec::parse("1in=1xyz").is_err());
    }
}
