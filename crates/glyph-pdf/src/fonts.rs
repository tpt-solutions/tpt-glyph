// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-pdf / fonts
//
// Baseline font/string handling. Decodes PDF text strings (PDFDocEncoding or
// UTF-16BE) into Unicode so text-layout operators (`Tj`, `TJ`, `Td`) can produce
// positioned glyphs. Per-glyph advances are obtained directly from the resolved
// `pdf::object::Font` in `content.rs`; full glyph-outline rasterization is a
// later milestone.

/// Decode a PDF string into Unicode text.
///
/// PDF text strings use either PDFDocEncoding (a superset of Latin-1) or
/// UTF-16BE when prefixed with a BOM. Other encodings fall back to lossy UTF-8
/// of the raw bytes.
pub fn decode_pdf_string(s: &pdf::primitive::PdfString) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE
        let mut out = String::new();
        let mut i = 2;
        while i + 1 < bytes.len() {
            let code = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
            out.push(char::from_u32(code as u32).unwrap_or('\u{FFFD}'));
            i += 2;
        }
        out
    } else {
        // PDFDocEncoding ≈ Latin-1 for the common range.
        bytes
            .iter()
            .map(|&b| match b {
                0x80..=0xFF => latin1_to_unicode(b),
                _ => b as char,
            })
            .collect()
    }
}

/// Map the PDFDocEncoding private-use range (0x80–0xFF) to Unicode. Only a
/// handful of common mappings are covered; the rest map to Latin-1.
fn latin1_to_unicode(b: u8) -> char {
    const MAP: &[(u8, char)] = &[
        (0x8E, 'Ž'),
        (0x9A, 'š'),
        (0x9E, 'ž'),
        (0x9F, 'Ÿ'),
        (0xA0, ' '),
        (0xAD, '‐'),
    ];
    for (code, ch) in MAP {
        if *code == b {
            return *ch;
        }
    }
    b as char
}
