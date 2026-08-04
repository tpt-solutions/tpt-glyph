// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-editor / content
//
// Serializes IR `Operation`s back into PDF content-stream bytes — the
// inverse of `tpt-glyph-pdf-parser`'s `convert_op`. Used when rebuilding a
// page's content stream after a `replace_text`/`insert_image` edit.

use tpt_glyph_pdf_ir::{Matrix, Operation, TextSegment};

/// Serialize a full sequence of content-stream operations.
pub fn write_operations(ops: &[Operation], out: &mut Vec<u8>) {
    for op in ops {
        write_operation(op, out);
    }
}

fn write_operation(op: &Operation, out: &mut Vec<u8>) {
    match op {
        Operation::Save => line(out, "q"),
        Operation::Restore => line(out, "Q"),
        Operation::ConcatMatrix(m) => matrix_op(out, m, "cm"),
        Operation::StrokeColorSpace(name) => name_op(out, name, "CS"),
        Operation::FillColorSpace(name) => name_op(out, name, "cs"),
        Operation::StrokeColor(v) => color_op(out, v, "SC", "G", "RG", "K"),
        Operation::FillColor(v) => color_op(out, v, "sc", "g", "rg", "k"),
        Operation::StrokeGray(g) => nums_op(out, &[*g], "G"),
        Operation::FillGray(g) => nums_op(out, &[*g], "g"),
        Operation::StrokeRgb(r, g, b) => nums_op(out, &[*r, *g, *b], "RG"),
        Operation::FillRgb(r, g, b) => nums_op(out, &[*r, *g, *b], "rg"),
        Operation::StrokeCmyk(c, m, y, k) => nums_op(out, &[*c, *m, *y, *k], "K"),
        Operation::FillCmyk(c, m, y, k) => nums_op(out, &[*c, *m, *y, *k], "k"),
        Operation::LineWidth(w) => nums_op(out, &[*w], "w"),
        Operation::LineCap(n) => nums_op(out, &[*n as f64], "J"),
        Operation::LineJoin(n) => nums_op(out, &[*n as f64], "j"),
        Operation::MiterLimit(m) => nums_op(out, &[*m], "M"),
        Operation::DashPattern(pattern, phase) => {
            out.push(b'[');
            for (i, p) in pattern.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_num(out, *p);
            }
            out.push(b']');
            out.push(b' ');
            write_num(out, *phase as f64);
            out.extend_from_slice(b" d\n");
        }
        Operation::RenderingIntent(name) => name_op(out, name, "ri"),
        Operation::Flatness(f) => nums_op(out, &[*f], "i"),
        Operation::SetGraphicsState(name) => name_op(out, name, "gs"),
        Operation::MoveTo(x, y) => nums_op(out, &[*x, *y], "m"),
        Operation::LineTo(x, y) => nums_op(out, &[*x, *y], "l"),
        Operation::CurveTo(x1, y1, x2, y2, x3, y3) => {
            nums_op(out, &[*x1, *y1, *x2, *y2, *x3, *y3], "c")
        }
        Operation::CurveToV(x2, y2, x3, y3) => nums_op(out, &[*x2, *y2, *x3, *y3], "v"),
        Operation::CurveToY(x1, y1, x3, y3) => nums_op(out, &[*x1, *y1, *x3, *y3], "y"),
        Operation::Rectangle(x, y, w, h) => nums_op(out, &[*x, *y, *w, *h], "re"),
        Operation::CloseSubpath => line(out, "h"),
        Operation::EndPath => line(out, "n"),
        Operation::Stroke => line(out, "S"),
        Operation::CloseAndStroke => line(out, "s"),
        Operation::Fill => line(out, "f"),
        Operation::FillEvenOdd => line(out, "f*"),
        Operation::FillAndStroke => line(out, "B"),
        Operation::FillAndStrokeEvenOdd => line(out, "B*"),
        Operation::CloseFillAndStroke => line(out, "b"),
        Operation::CloseFillAndStrokeEvenOdd => line(out, "b*"),
        Operation::Clip => line(out, "W"),
        Operation::ClipEvenOdd => line(out, "W*"),
        Operation::BeginText => line(out, "BT"),
        Operation::EndText => line(out, "ET"),
        Operation::CharSpacing(v) => nums_op(out, &[*v], "Tc"),
        Operation::WordSpacing(v) => nums_op(out, &[*v], "Tw"),
        Operation::TextScaling(v) => nums_op(out, &[*v], "Tz"),
        Operation::Leading(v) => nums_op(out, &[*v], "TL"),
        Operation::TextFont(name, size) => {
            out.push(b'/');
            write_name(out, name);
            out.push(b' ');
            write_num(out, *size);
            out.extend_from_slice(b" Tf\n");
        }
        Operation::TextRenderMode(n) => nums_op(out, &[*n as f64], "Tr"),
        Operation::TextRise(v) => nums_op(out, &[*v], "Ts"),
        Operation::MoveTextPosition(x, y) => nums_op(out, &[*x, *y], "Td"),
        Operation::MoveTextPositionAndLeading(x, y) => nums_op(out, &[*x, *y], "TD"),
        Operation::SetTextMatrix(m) => matrix_op(out, m, "Tm"),
        Operation::TextNewline => line(out, "T*"),
        Operation::TextDraw(bytes) => {
            write_pdf_string(out, bytes);
            out.extend_from_slice(b" Tj\n");
        }
        Operation::TextDrawAdjusted(segments) => {
            out.push(b'[');
            for seg in segments {
                match seg {
                    TextSegment::Text(bytes) => write_pdf_string(out, bytes),
                    TextSegment::Spacing(dx) => write_num(out, *dx),
                }
                out.push(b' ');
            }
            out.extend_from_slice(b"] TJ\n");
        }
        Operation::TextNewlineAndDraw(bytes) => {
            write_pdf_string(out, bytes);
            out.extend_from_slice(b" '\n");
        }
        Operation::TextNewlineWithSpacingAndDraw(aw, ac, bytes) => {
            write_num(out, *aw);
            out.push(b' ');
            write_num(out, *ac);
            out.push(b' ');
            write_pdf_string(out, bytes);
            out.extend_from_slice(b" \"\n");
        }
        Operation::BeginMarkedContent(tag) => name_op(out, tag, "BMC"),
        // The original marked-content property dict isn't retained by the
        // IR (see `tpt-glyph-pdf-parser::convert_op`), so it round-trips as
        // an empty inline dict here — valid content-stream syntax, just
        // without whatever properties the source PDF supplied.
        Operation::BeginMarkedContentWithProps(tag) => {
            out.push(b'/');
            write_name(out, tag);
            out.extend_from_slice(b" <<>> BDC\n");
        }
        Operation::EndMarkedContent => line(out, "EMC"),
        Operation::MarkedContentPoint(tag) => name_op(out, tag, "MP"),
        Operation::MarkedContentPointWithProps(tag) => {
            out.push(b'/');
            write_name(out, tag);
            out.extend_from_slice(b" <<>> DP\n");
        }
        Operation::PaintXObject(name) => name_op(out, name, "Do"),
        Operation::Shade(name) => name_op(out, name, "sh"),
        Operation::InlineImage(dict, data) => {
            out.extend_from_slice(b"BI\n");
            for (key, value) in dict {
                out.push(b'/');
                write_name(out, key);
                out.push(b' ');
                write_inline_image_value(out, value);
                out.push(b'\n');
            }
            out.extend_from_slice(b"ID\n");
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nEI\n");
        }
        Operation::SetCharWidth(wx, wy) => nums_op(out, &[*wx, *wy], "d0"),
        Operation::SetCacheDevice(wx, wy, llx, lly, urx, ury, ax, ay) => {
            nums_op(out, &[*wx, *wy, *llx, *lly, *urx, *ury, *ax, *ay], "d1")
        }
    }
}

fn write_inline_image_value(out: &mut Vec<u8>, value: &tpt_glyph_pdf_ir::PdfValue) {
    use tpt_glyph_pdf_ir::PdfValue;
    match value {
        PdfValue::Integer(i) => out.extend_from_slice(i.to_string().as_bytes()),
        PdfValue::Real(f) => write_num(out, *f),
        PdfValue::Name(n) => {
            out.push(b'/');
            write_name(out, n);
        }
        PdfValue::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        _ => out.extend_from_slice(b"null"),
    }
}

fn line(out: &mut Vec<u8>, op: &str) {
    out.extend_from_slice(op.as_bytes());
    out.push(b'\n');
}

fn nums_op(out: &mut Vec<u8>, nums: &[f64], op: &str) {
    for n in nums {
        write_num(out, *n);
        out.push(b' ');
    }
    out.extend_from_slice(op.as_bytes());
    out.push(b'\n');
}

/// Emit a generic `SC`/`sc` color operator using the operand-count-specific
/// operator instead whenever the count unambiguously matches DeviceGray (1),
/// DeviceRGB (3), or DeviceCMYK (4).
///
/// This matters beyond style: the `pdf` crate (used by the rendering
/// pipeline) classifies a color by which operator set it, not just by
/// operand count — `sc`/`SC` with no active `cs`/`CS` colorspace decodes to
/// an unclassified `Color::Other`, which the renderer treats as opaque
/// black, while `rg`/`g`/`k` are unambiguous. Emitting the specific operator
/// keeps re-parsed colors correct instead of silently turning black.
fn color_op(out: &mut Vec<u8>, nums: &[f64], generic: &str, gray: &str, rgb: &str, cmyk: &str) {
    match nums.len() {
        0 => {}
        1 => nums_op(out, nums, gray),
        3 => nums_op(out, nums, rgb),
        4 => nums_op(out, nums, cmyk),
        _ => nums_op(out, nums, generic),
    }
}

fn matrix_op(out: &mut Vec<u8>, m: &Matrix, op: &str) {
    nums_op(out, &[m.a, m.b, m.c, m.d, m.e, m.f], op);
}

fn name_op(out: &mut Vec<u8>, name: &str, op: &str) {
    out.push(b'/');
    write_name(out, name);
    out.push(b' ');
    out.extend_from_slice(op.as_bytes());
    out.push(b'\n');
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    // Names round-trip verbatim; the small set of PDF delimiter characters
    // isn't expected to appear in the names this crate produces (drawn from
    // resource dictionary keys the parser itself generated), so no `#xx`
    // escaping is attempted here.
    out.extend_from_slice(name.as_bytes());
}

fn write_num(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(format!("{v}").as_bytes());
}

/// Write `bytes` as a PDF literal string `(...)`, escaping `(`, `)`, and `\`.
fn write_pdf_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(b'(');
    for &b in bytes {
        match b {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            _ => out.push(b),
        }
    }
    out.push(b')');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_simple_path_fill() {
        let ops = vec![
            Operation::FillRgb(1.0, 0.0, 0.0),
            Operation::Rectangle(10.0, 10.0, 80.0, 60.0),
            Operation::Fill,
        ];
        let mut out = Vec::new();
        write_operations(&ops, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "1 0 0 rg\n10 10 80 60 re\nf\n");
    }

    #[test]
    fn escapes_parens_and_backslash_in_text_draw() {
        let ops = vec![Operation::TextDraw(b"a (b) c\\d".to_vec())];
        let mut out = Vec::new();
        write_operations(&ops, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "(a \\(b\\) c\\\\d) Tj\n");
    }

    #[test]
    fn text_draw_adjusted_mixes_strings_and_spacing() {
        let ops = vec![Operation::TextDrawAdjusted(vec![
            TextSegment::Text(b"AB".to_vec()),
            TextSegment::Spacing(-120.0),
            TextSegment::Text(b"C".to_vec()),
        ])];
        let mut out = Vec::new();
        write_operations(&ops, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "[(AB) -120 (C) ] TJ\n");
    }
}
