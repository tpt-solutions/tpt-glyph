// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — out-glyph-measure
//
// Scaled line-measurement over a PDF page: reuses
// `tpt-glyph-pdf-measure`'s geometry primitives to find painted paths, then
// applies a per-page drawing scale (`scale`) to convert PDF-unit lengths
// into real-world units.

pub mod scale;

pub use scale::{LengthUnit, ScaleSpec, ScaleTable};
use tpt_glyph_pdf_measure::{total_length, PaintKind, PaintedPath};

/// One measured path on a page: its position in `painted_paths`' output,
/// what kind of paint produced it, its length in PDF units, and that same
/// length converted to real-world millimeters under the page's scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub path_index: usize,
    pub kind: PaintKind,
    pub pdf_length: f64,
    pub real_world_mm: f64,
}

/// Measure every painted path on `page`, applying `scale`.
pub fn measure_page(page: &tpt_glyph_pdf_ir::Page, scale: ScaleSpec) -> Vec<Measurement> {
    tpt_glyph_pdf_measure::painted_paths(page)
        .into_iter()
        .enumerate()
        .map(|(path_index, path)| {
            let pdf_length = total_length(std::slice::from_ref(&path));
            Measurement {
                path_index,
                kind: path.kind,
                pdf_length,
                real_world_mm: scale.real_world_mm(pdf_length),
            }
        })
        .collect()
}

/// Measure a single path (by its index in `painted_paths`' output order).
/// Returns `None` if `path_index` is out of range.
pub fn measure_path(
    page: &tpt_glyph_pdf_ir::Page,
    path_index: usize,
    scale: ScaleSpec,
) -> Option<Measurement> {
    let paths: Vec<PaintedPath> = tpt_glyph_pdf_measure::painted_paths(page);
    let path = paths.get(path_index)?;
    let pdf_length = total_length(std::slice::from_ref(path));
    Some(Measurement {
        path_index,
        kind: path.kind,
        pdf_length,
        real_world_mm: scale.real_world_mm(pdf_length),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_glyph_pdf_ir::{ContentStream, Operation, Page, Rect, Resources};

    fn page_with_line(x0: f64, y0: f64, x1: f64, y1: f64) -> Page {
        Page {
            index: 0,
            label: None,
            media_box: Rect::new(0.0, 0.0, 1000.0, 1000.0),
            crop_box: None,
            bleed_box: None,
            trim_box: None,
            art_box: None,
            rotate: 0,
            resources: Resources::empty(),
            contents: vec![ContentStream::new(vec![
                Operation::MoveTo(x0, y0),
                Operation::LineTo(x1, y1),
                Operation::Stroke,
            ])],
            annotations: Vec::new(),
            thumb: None,
            struct_parents: None,
        }
    }

    #[test]
    fn measures_known_line_length_in_pdf_units() {
        // A 3-4-5 triangle: line length is exactly 50 PDF units.
        let page = page_with_line(0.0, 0.0, 30.0, 40.0);
        let measurements = measure_page(&page, ScaleSpec::IDENTITY);
        assert_eq!(measurements.len(), 1);
        assert!((measurements[0].pdf_length - 50.0).abs() < 1e-9);
    }

    #[test]
    fn applies_scale_to_produce_real_world_length() {
        // 72 PDF units (1 inch on paper) at 1:100 -> 2540mm real-world.
        let page = page_with_line(0.0, 0.0, 72.0, 0.0);
        let scale = ScaleSpec::parse("1:100").unwrap();
        let m = measure_path(&page, 0, scale).unwrap();
        assert!(
            (m.real_world_mm - 2540.0).abs() < 1e-6,
            "got {}",
            m.real_world_mm
        );
    }

    #[test]
    fn out_of_range_path_index_is_none() {
        let page = page_with_line(0.0, 0.0, 10.0, 0.0);
        assert!(measure_path(&page, 5, ScaleSpec::IDENTITY).is_none());
    }
}
