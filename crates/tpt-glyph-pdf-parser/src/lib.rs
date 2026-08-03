// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-parser
//
// Robust PDF parsing into the canonical `tpt-glyph-pdf-ir` intermediate
// representation. This crate is the migration path for the parsing role that
// historically lived in `tpt-glyph-pdf`: parse a PDF into the immutable IR,
// and let downstream crates (rendering, editing, measuring) operate on that
// IR instead of reaching into the `pdf` crate's object model.

use tpt_glyph_pdf_ir as ir;
use tpt_glyph_pdf_ir::{ContentStream, Operation, Page, Rect, Resources};

use pdf::content::{Color, Matrix as PdfMatrix, TextDrawAdjusted as PdfTextDrawAdjusted, TextMode};
use pdf::content::{LineCap as PdfLineCap, LineJoin as PdfLineJoin, Winding};
use pdf::error::PdfError;
use pdf::font::{FontData, FontType};
use pdf::object::{
    Annot as PdfAnnot, ColorSpace as PdfColorSpace, Counter, MaybeRef, Object, PageLabel,
    Rectangle,
};
use pdf::primitive::{Dictionary, Name, Primitive};

/// Concrete file type produced by `FileOptions::uncached().load(data)`.
type PdfFile<'a> =
    pdf::file::File<&'a [u8], pdf::file::NoCache, pdf::file::NoCache, pdf::file::NoLog>;

/// Errors produced while parsing a PDF into the IR.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("failed to open/parse PDF: {0}")]
    Pdf(#[from] PdfError),
    #[error("malformed document: {0}")]
    Malformed(String),
    #[error("unsupported feature: {0}")]
    Unsupported(String),
}

pub type Result<T> = core::result::Result<T, ParseError>;

/// Parse PDF bytes into the canonical immutable IR.
pub fn parse_bytes(data: &[u8]) -> Result<ir::Document> {
    let file = pdf::file::FileOptions::uncached().load(data)?;
    let labels = collect_page_labels(&file);
    let mut pages = Vec::new();
    for idx in 0..file.num_pages() {
        let page = convert_page(&file, idx, &labels)?;
        pages.push(page);
    }

    let objects = scan_objects(&file);
    let trailer = convert_trailer(&file, &objects);
    let xref = ir::XRef {
        entries: compute_xref_entries(data, &objects),
    };
    let version = file.version().unwrap_or_else(|_| "1.7".into());

    Ok(ir::Document {
        version,
        pages,
        xref,
        trailer,
        objects,
    })
}

/// Parse a PDF from a file path into the canonical immutable IR.
pub fn parse_path(path: impl AsRef<std::path::Path>) -> Result<ir::Document> {
    let data = std::fs::read(path.as_ref()).map_err(|e| ParseError::Malformed(e.to_string()))?;
    parse_bytes(&data)
}

// ---------------------------------------------------------------------------
// Trailer
// ---------------------------------------------------------------------------

fn convert_trailer(file: &PdfFile<'_>, objects: &[(u32, u16, ir::PdfValue)]) -> ir::Trailer {
    let root_id = file.trailer.root.get_ref().get_inner().id;
    let id = (file.trailer.id.len() >= 2).then(|| {
        [
            String::from_utf8_lossy(file.trailer.id[0].as_bytes()).to_string(),
            String::from_utf8_lossy(file.trailer.id[1].as_bytes()).to_string(),
        ]
    });
    ir::Trailer {
        root: root_id as u32,
        info: find_info_object_number(file, objects),
        id,
        encrypt: file.trailer.encrypt_dict.as_ref().map(|rc| {
            let inner = rc.get_ref().get_inner();
            ir::PdfValue::Reference(inner.id as u32, inner.gen as u16)
        }),
    }
}

/// The typed `Trailer` discards the /Info reference, so recover the object
/// number by matching the parsed `InfoDict` fields against the scanned
/// object inventory. Falls back to `None` for documents without a /Info dict
/// or where the match is ambiguous.
fn find_info_object_number(
    file: &PdfFile<'_>,
    objects: &[(u32, u16, ir::PdfValue)],
) -> Option<u32> {
    let info = file.trailer.info_dict.as_ref()?;
    let mut signature: Vec<(&str, Vec<u8>)> = Vec::new();
    macro_rules! push {
        ($key:literal, $field:ident) => {
            if let Some(v) = &info.$field {
                signature.push(($key, v.as_bytes().to_vec()));
            }
        };
    }
    push!("Title", title);
    push!("Author", author);
    push!("Subject", subject);
    push!("Keywords", keywords);
    push!("Creator", creator);
    push!("Producer", producer);
    if signature.is_empty() {
        return None;
    }
    objects.iter().find_map(|&(num, _, ref value)| {
        let ir::PdfValue::Dict(entries) = value else {
            return None;
        };
        let matches = signature.iter().all(|(key, expected)| {
            entries.iter().any(|(entry_key, entry_value)| {
                entry_key == key
                    && matches!(entry_value, ir::PdfValue::String(s) if s == expected)
            })
        });
        matches.then_some(num)
    })
}

// ---------------------------------------------------------------------------
// Object inventory + xref reconstruction
// ---------------------------------------------------------------------------

/// Enumerate all indirect objects reachable in the file body. Objects stored
/// inside compressed object streams are not surfaced by the `pdf` crate's
/// scan pass; for those files the `objects` list is best-effort (still
/// complete for classic-xref documents).
fn scan_objects(file: &PdfFile<'_>) -> Vec<(u32, u16, ir::PdfValue)> {
    let resolver = file.resolver();
    let mut objects = Vec::new();
    for item in file.scan() {
        if let Ok(pdf::file::ScanItem::Object(plain_ref, primitive)) = item {
            objects.push((
                plain_ref.id as u32,
                plain_ref.gen as u16,
                primitive_to_ir_value(primitive, &resolver),
            ));
        }
    }
    objects
}

/// The xref table is indexed by object number; `entries[0]` is the mandatory
/// free-head entry.
fn compute_xref_entries(data: &[u8], objects: &[(u32, u16, ir::PdfValue)]) -> Vec<ir::XRefEntry> {
    let max_num = objects.iter().map(|&(n, _, _)| n).max().unwrap_or(0);
    let mut entries = vec![ir::XRefEntry::new(0, 65535, false); max_num as usize + 1];
    for (&(num, gen, _), offset) in objects.iter().zip(compute_object_offsets(data, objects)) {
        if let Some(entry) = entries.get_mut(num as usize) {
            *entry = ir::XRefEntry::new(offset, gen, true);
        }
    }
    entries
}

/// Best-effort byte offsets for a list of `(object_number, generation)`
/// pairs, found by scanning the raw body for each `N G obj` header in file
/// order. Correct for classic-xref documents; incremental-update files may
/// resolve to the first (superseded) occurrence of a redefined object.
fn compute_object_offsets(data: &[u8], objects: &[(u32, u16, ir::PdfValue)]) -> Vec<u64> {
    let mut offsets = Vec::with_capacity(objects.len());
    let mut pos = 0usize;
    for &(id, gen, _) in objects {
        let marker = format!("\n{id} {gen} obj");
        let marker = marker.as_bytes();
        let mut found = None;
        let mut p = pos;
        while p < data.len() {
            if let Some(abs) = find_bytes(data, marker, p) {
                // xref offsets point at the `N G obj` header itself.
                found = Some(abs + 1);
                pos = abs + marker.len();
                break;
            }
            match data[p..].iter().position(|&b| b == b'\n') {
                Some(nl) => p += nl + 1,
                None => break,
            }
        }
        offsets.push(found.unwrap_or(0) as u64);
    }
    offsets
}

fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < from + needle.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

/// Convert a `pdf` crate primitive into the IR's lossless `PdfValue`.
fn primitive_to_ir_value(p: Primitive, resolver: &impl pdf::object::Resolve) -> ir::PdfValue {
    match p {
        Primitive::Null => ir::PdfValue::Null,
        Primitive::Integer(i) => ir::PdfValue::Integer(i as i64),
        Primitive::Number(f) => ir::PdfValue::Real(f as f64),
        Primitive::Boolean(b) => ir::PdfValue::Boolean(b),
        Primitive::String(s) => ir::PdfValue::String(s.as_bytes().to_vec()),
        Primitive::Stream(stream) => ir::PdfValue::Stream(ir::PdfStream {
            dict: dict_to_ir(&stream.info, resolver),
            data: stream.raw_data(resolver).unwrap_or_default().to_vec(),
        }),
        Primitive::Dictionary(d) => ir::PdfValue::Dict(dict_to_ir(&d, resolver)),
        Primitive::Array(a) => ir::PdfValue::Array(
            a.into_iter()
                .map(|p| primitive_to_ir_value(p, resolver))
                .collect(),
        ),
        Primitive::Reference(r) => ir::PdfValue::Reference(r.id as u32, r.gen as u16),
        Primitive::Name(n) => ir::PdfValue::Name(name_str(&Name(n))),
    }
}

fn dict_to_ir(d: &Dictionary, resolver: &impl pdf::object::Resolve) -> ir::PdfDict {
    d.iter()
        .map(|(k, v)| (name_str(k), primitive_to_ir_value(v.clone(), resolver)))
        .collect()
}

// ---------------------------------------------------------------------------
// Page tree
// ---------------------------------------------------------------------------

fn convert_page(file: &PdfFile<'_>, index: u32, labels: &[PageLabelRange]) -> Result<Page> {
    let resolver = file.resolver();
    let page_rc = file.get_page(index)?;
    let page = &*page_rc;

    let rect = page.media_box()?;
    let media_box = Rect::new(
        rect.left as f64,
        rect.bottom as f64,
        rect.right as f64,
        rect.top as f64,
    );

    let crop_box = page
        .crop_box()
        .ok()
        .map(|r| Rect::new(r.left as f64, r.bottom as f64, r.right as f64, r.top as f64));
    let trim_box = page
        .trim_box
        .map(|r| Rect::new(r.left as f64, r.bottom as f64, r.right as f64, r.top as f64));
    let bleed_box = page
        .other
        .get("BleedBox")
        .and_then(|p| Rectangle::from_primitive(p.clone(), &resolver).ok())
        .map(|r| Rect::new(r.left as f64, r.bottom as f64, r.right as f64, r.top as f64));
    let art_box = page
        .other
        .get("ArtBox")
        .and_then(|p| Rectangle::from_primitive(p.clone(), &resolver).ok())
        .map(|r| Rect::new(r.left as f64, r.bottom as f64, r.right as f64, r.top as f64));

    let resources = convert_resources(page, &resolver);
    let contents = convert_contents(page, &resolver);

    Ok(Page {
        index,
        label: page_label(index, labels),
        media_box,
        crop_box,
        bleed_box,
        trim_box,
        art_box,
        rotate: page.rotate,
        resources,
        contents,
        annotations: convert_annotations(page, &resolver),
        thumb: None,
        struct_parents: None,
    })
}

// ---------------------------------------------------------------------------
// Page labels
// ---------------------------------------------------------------------------

/// One range of the document's /PageLabels number tree.
struct PageLabelRange {
    start: u32,
    prefix: String,
    style: Option<Counter>,
    counter_start: u32,
}

/// Flatten the catalog's /PageLabels tree into sorted, non-overlapping ranges
/// (each range applies from its start index until the next range's start).
fn collect_page_labels(file: &PdfFile<'_>) -> Vec<PageLabelRange> {
    let mut ranges = Vec::new();
    if let Some(tree) = &file.trailer.root.page_labels {
        let resolver = file.resolver();
        let _ = tree.walk(&resolver, &mut |idx, label: &PageLabel| {
            ranges.push(PageLabelRange {
                start: idx.max(0) as u32,
                prefix: label
                    .prefix
                    .as_ref()
                    .map(|s| String::from_utf8_lossy(s.as_bytes()).to_string())
                    .unwrap_or_default(),
                style: label.style.clone(),
                counter_start: label.start.unwrap_or(1).max(1) as u32,
            });
        });
    }
    ranges.sort_by_key(|r| r.start);
    ranges
}

fn page_label(index: u32, ranges: &[PageLabelRange]) -> Option<String> {
    let range = ranges.iter().rev().find(|r| r.start <= index)?;
    let value = index as u64 - range.start as u64 + range.counter_start as u64;
    let numbered = match range.style {
        None => String::new(),
        Some(Counter::Arabic) => value.to_string(),
        // Note: the `pdf` crate's enum variant names are case-inverted
        // relative to the PDF spec. A file `/S /r` (spec: lowercase roman)
        // parses to `Counter::RomanUpper`, so we render lower/upper here to
        // match what the spec actually means.
        Some(Counter::RomanUpper) => to_roman(value),
        Some(Counter::RomanLower) => to_roman(value).to_uppercase(),
        Some(Counter::AlphaUpper) => to_alpha(value),
        Some(Counter::AlphaLower) => to_alpha(value).to_uppercase(),
    };
    Some(format!("{}{}", range.prefix, numbered))
}

fn to_roman(mut n: u64) -> String {
    const TABLE: [(u64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(value, numeral) in &TABLE {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

fn to_alpha(mut n: u64) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    out
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

fn convert_annotations(
    page: &pdf::object::Page,
    resolver: &impl pdf::object::Resolve,
) -> Vec<ir::Annotation> {
    match page.annotations.load(resolver) {
        Ok(annots) => annots
            .data()
            .iter()
            .filter_map(|a| convert_annotation(a))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn convert_annotation(a: &MaybeRef<PdfAnnot>) -> Option<ir::Annotation> {
    let ann = a.data();
    let rect = ann.rect.map(|r| Rect::new(r.left as f64, r.bottom as f64, r.right as f64, r.top as f64));
    let subtype = name_str(&ann.subtype);
    let contents = ann
        .contents
        .as_ref()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).to_string());
    let rect_value = rect.map(|r| {
        ir::PdfValue::Array(vec![
            ir::PdfValue::Real(r.left),
            ir::PdfValue::Real(r.bottom),
            ir::PdfValue::Real(r.right),
            ir::PdfValue::Real(r.top),
        ])
    });

    let mut value: ir::PdfDict = vec![("Subtype".into(), ir::PdfValue::Name(subtype.clone()))];
    if let Some(r) = rect_value {
        value.push(("Rect".into(), r));
    }
    if let Some(c) = &ann.contents {
        value.push(("Contents".into(), ir::PdfValue::String(c.as_bytes().to_vec())));
    }

    Some(ir::Annotation {
        subtype,
        rect: rect.unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0)),
        contents,
        value: ir::PdfValue::Dict(value),
    })
}

fn convert_contents(
    page: &pdf::object::Page,
    resolver: &impl pdf::object::Resolve,
) -> Vec<ContentStream> {
    match &page.contents {
        Some(content) => match content.operations(resolver) {
            Ok(ops) => {
                let ir_ops = ops.iter().map(|op| convert_op(op, resolver)).collect();
                vec![ContentStream::new(ir_ops)]
            }
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn convert_resources(page: &pdf::object::Page, resolver: &impl pdf::object::Resolve) -> Resources {
    match page.resources() {
        Ok(resources) => convert_resources_from(resources, resolver),
        Err(_) => Resources::empty(),
    }
}

/// Names in the `pdf` crate carry a leading slash; the IR stores bare names.
fn name_str(n: &Name) -> String {
    n.0.trim_start_matches('/').to_string()
}

fn convert_resources_from(
    res: &pdf::object::Resources,
    resolver: &impl pdf::object::Resolve,
) -> Resources {
    let mut out = Resources::empty();

    // Fonts.
    for (name, font) in &res.fonts {
        let font_ir = font
            .load(resolver)
            .map(|f| convert_font(&f))
            .unwrap_or_else(|_| default_font_ref());
        out.fonts.push((name_str(name), font_ir));
    }

    // XObjects (best-effort: resolve form XObjects into their op lists).
    for (name, xobj) in &res.xobjects {
        let converted = match resolver.get(*xobj) {
            Ok(xobj_ref) => convert_xobject(&xobj_ref, resolver),
            Err(_) => {
                let inner = xobj.get_inner();
                ir::XObject::Reference(ir::PdfValue::Reference(inner.id as u32, inner.gen as u16))
            }
        };
        out.xobjects.push((name_str(name), converted));
    }

    // ExtGState entries.
    for (name, gs) in &res.graphics_states {
        out.ext_gstates.push((
            name_str(name),
            ir::ExtGState {
                line_width: gs.line_width.map(|w| w as f64),
                line_cap: gs.line_cap.map(|c| c as u8),
                line_join: gs.line_join.map(|j| j as u8),
                miter_limit: gs.miter_limit.map(|m| m as f64),
                dash_pattern: gs.dash_pattern.as_ref().map(|d| {
                    let pattern = d
                        .first()
                        .and_then(|p| p.as_array().ok())
                        .map(|arr| arr.iter().filter_map(primitive_to_f64).collect())
                        .unwrap_or_default();
                    let phase = d
                        .get(1)
                        .and_then(|p| p.as_number().ok())
                        .unwrap_or(0.0)
                        .max(0.0) as u64;
                    (pattern, phase)
                }),
                rendering_intent: gs.rendering_intent.as_ref().map(name_str),
                alpha_stroke: gs.stroke_alpha.map(|a| a as f64),
                alpha_fill: gs.fill_alpha.map(|a| a as f64),
                blend_mode: None,
                soft_mask: None,
                stroke_adjust: None,
                overprint_stroke: gs.overprint,
                overprint_fill: gs.overprint_fill,
                overprint_mode: gs.overprint_mode.map(|m| m as u8),
                transfer: None,
                halftone: None,
                flatness: None,
                smoothness: None,
            },
        ));
    }

    // Color spaces (best-effort name-only mapping).
    for (name, cs) in &res.color_spaces {
        out.color_spaces
            .push((name_str(name), ir::PdfValue::Name(color_space_name(cs))));
    }

    // Patterns are kept as unresolved references.
    for (name, pat) in &res.pattern {
        let inner = pat.get_inner();
        out.patterns.push((
            name_str(name),
            ir::PdfValue::Reference(inner.id as u32, inner.gen as u16),
        ));
    }

    out
}

fn default_font_ref() -> ir::FontRef {
    ir::FontRef {
        subtype: String::new(),
        base_font: String::new(),
        first_char: 0,
        last_char: 0,
        widths: Vec::new(),
        descriptor: None,
        to_unicode: None,
        encoding: None,
    }
}

fn convert_font(font: &pdf::font::Font) -> ir::FontRef {
    let subtype = match font.subtype {
        FontType::Type0 => "Type0",
        FontType::Type1 => "Type1",
        FontType::MMType1 => "MMType1",
        FontType::Type3 => "Type3",
        FontType::TrueType => "TrueType",
        FontType::CIDFontType0 => "CIDFontType0",
        FontType::CIDFontType2 => "CIDFontType2",
    }
    .to_string();

    let base_font = font.name.as_ref().map(name_str).unwrap_or_default();

    let (first_char, last_char, widths) = match &font.data {
        FontData::Type1(tf) | FontData::TrueType(tf) => (
            tf.first_char.unwrap_or(0).max(0) as u32,
            tf.last_char.unwrap_or(0).max(0) as u32,
            tf.widths
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|w| w as f64)
                .collect(),
        ),
        FontData::CIDFontType0(cid) | FontData::CIDFontType2(cid) => (
            0,
            0,
            cid.widths.iter().filter_map(primitive_to_f64).collect(),
        ),
        _ => (0, 0, Vec::new()),
    };

    let to_unicode = font.to_unicode.as_ref().map(|rc| {
        let inner = rc.get_ref().get_inner();
        ir::PdfValue::Reference(inner.id as u32, inner.gen as u16)
    });

    ir::FontRef {
        subtype,
        base_font,
        first_char,
        last_char,
        widths,
        descriptor: None,
        to_unicode,
        encoding: None,
    }
}

fn convert_xobject(
    xobj: &pdf::object::XObject,
    resolver: &impl pdf::object::Resolve,
) -> ir::XObject {
    use pdf::object::XObject;
    match xobj {
        XObject::Form(form) => {
            let dict = form.dict();
            let bbox = Rect::new(
                dict.bbox.left as f64,
                dict.bbox.bottom as f64,
                dict.bbox.right as f64,
                dict.bbox.top as f64,
            );
            let matrix = dict
                .matrix
                .as_ref()
                .and_then(|p| PdfMatrix::from_primitive(p.clone(), resolver).ok())
                .map(|m| ir::Matrix {
                    a: m.a as f64,
                    b: m.b as f64,
                    c: m.c as f64,
                    d: m.d as f64,
                    e: m.e as f64,
                    f: m.f as f64,
                })
                .unwrap_or(ir::Matrix::IDENTITY);
            let ops = form
                .operations(resolver)
                .map(|ops| ops.iter().map(|op| convert_op(op, resolver)).collect())
                .unwrap_or_default();
            let resources = match &dict.resources {
                Some(r) => convert_resources_from(r, resolver),
                None => Resources::empty(),
            };
            ir::XObject::Form {
                bbox,
                matrix,
                resources,
                ops,
            }
        }
        XObject::Image(img) => ir::XObject::Image {
            width: img.width,
            height: img.height,
            bits_per_component: img.bits_per_component.unwrap_or(8).clamp(1, 255) as u8,
            color_space: img
                .color_space
                .as_ref()
                .map(color_space_name)
                .unwrap_or_default(),
            data: img.image_data(resolver).unwrap_or_default().to_vec(),
            mask: None,
            smask: None,
        },
        XObject::Postscript(_) => ir::XObject::Reference(ir::PdfValue::Null),
    }
}

// ---------------------------------------------------------------------------
// Content operators
// ---------------------------------------------------------------------------

use pdf::content::Op as PdfOp;

fn convert_op(op: &PdfOp, resolver: &impl pdf::object::Resolve) -> Operation {
    match op {
        PdfOp::Save => Operation::Save,
        PdfOp::Restore => Operation::Restore,
        PdfOp::Transform { matrix } => Operation::ConcatMatrix(to_ir_matrix(matrix)),
        PdfOp::LineWidth { width } => Operation::LineWidth(*width as f64),
        PdfOp::LineJoin { join } => Operation::LineJoin(match join {
            PdfLineJoin::Miter => 0,
            PdfLineJoin::Round => 1,
            PdfLineJoin::Bevel => 2,
        }),
        PdfOp::LineCap { cap } => Operation::LineCap(match cap {
            PdfLineCap::Butt => 0,
            PdfLineCap::Round => 1,
            PdfLineCap::Square => 2,
        }),
        PdfOp::MiterLimit { limit } => Operation::MiterLimit(*limit as f64),
        PdfOp::Dash { pattern, phase } => {
            Operation::DashPattern(pattern.iter().map(|x| *x as f64).collect(), *phase as u64)
        }
        PdfOp::Flatness { tolerance } => Operation::Flatness(*tolerance as f64),
        PdfOp::GraphicsState { name } => Operation::SetGraphicsState(name_str(name)),
        PdfOp::StrokeColor { color } => Operation::StrokeColor(color_vec(color)),
        PdfOp::FillColor { color } => Operation::FillColor(color_vec(color)),
        PdfOp::StrokeColorSpace { name } => Operation::StrokeColorSpace(name_str(name)),
        PdfOp::FillColorSpace { name } => Operation::FillColorSpace(name_str(name)),
        PdfOp::RenderingIntent { intent } => {
            Operation::RenderingIntent(intent.to_str().to_string())
        }
        PdfOp::MoveTo { p } => Operation::MoveTo(p.x as f64, p.y as f64),
        PdfOp::LineTo { p } => Operation::LineTo(p.x as f64, p.y as f64),
        PdfOp::CurveTo { c1, c2, p } => Operation::CurveTo(
            c1.x as f64,
            c1.y as f64,
            c2.x as f64,
            c2.y as f64,
            p.x as f64,
            p.y as f64,
        ),
        PdfOp::Rect { rect } => Operation::Rectangle(
            rect.x as f64,
            rect.y as f64,
            rect.width as f64,
            rect.height as f64,
        ),
        PdfOp::Close => Operation::CloseSubpath,
        PdfOp::EndPath => Operation::EndPath,
        PdfOp::Stroke => Operation::Stroke,
        PdfOp::Fill { winding } => match winding {
            Winding::NonZero => Operation::Fill,
            Winding::EvenOdd => Operation::FillEvenOdd,
        },
        PdfOp::FillAndStroke { winding } => match winding {
            Winding::NonZero => Operation::FillAndStroke,
            Winding::EvenOdd => Operation::FillAndStrokeEvenOdd,
        },
        PdfOp::Clip { winding } => match winding {
            Winding::NonZero => Operation::Clip,
            Winding::EvenOdd => Operation::ClipEvenOdd,
        },
        PdfOp::BeginText => Operation::BeginText,
        PdfOp::EndText => Operation::EndText,
        PdfOp::CharSpacing { char_space } => Operation::CharSpacing(*char_space as f64),
        PdfOp::WordSpacing { word_space } => Operation::WordSpacing(*word_space as f64),
        PdfOp::TextScaling { horiz_scale } => Operation::TextScaling(*horiz_scale as f64),
        PdfOp::Leading { leading } => Operation::Leading(*leading as f64),
        PdfOp::TextFont { name, size } => Operation::TextFont(name_str(name), *size as f64),
        PdfOp::TextRenderMode { mode } => Operation::TextRenderMode(match mode {
            TextMode::Fill => 0,
            TextMode::Stroke => 1,
            TextMode::FillThenStroke => 2,
            TextMode::Invisible => 3,
            TextMode::FillAndClip => 4,
            TextMode::StrokeAndClip => 5,
        }),
        PdfOp::TextRise { rise } => Operation::TextRise(*rise as f64),
        PdfOp::MoveTextPosition { translation } => {
            Operation::MoveTextPosition(translation.x as f64, translation.y as f64)
        }
        PdfOp::SetTextMatrix { matrix } => Operation::SetTextMatrix(to_ir_matrix(matrix)),
        PdfOp::TextNewline => Operation::TextNewline,
        PdfOp::TextDraw { text } => Operation::TextDraw(text.as_bytes().to_vec()),
        PdfOp::TextDrawAdjusted { array } => Operation::TextDrawAdjusted(
            array
                .iter()
                .map(|seg| match seg {
                    PdfTextDrawAdjusted::Text(t) => ir::TextSegment::Text(t.as_bytes().to_vec()),
                    PdfTextDrawAdjusted::Spacing(dx) => ir::TextSegment::Spacing(*dx as f64),
                })
                .collect(),
        ),
        PdfOp::BeginMarkedContent { tag, properties } => {
            if properties.is_some() {
                Operation::BeginMarkedContentWithProps(name_str(tag))
            } else {
                Operation::BeginMarkedContent(name_str(tag))
            }
        }
        PdfOp::EndMarkedContent => Operation::EndMarkedContent,
        PdfOp::MarkedContentPoint { tag, properties } => {
            if properties.is_some() {
                Operation::MarkedContentPointWithProps(name_str(tag))
            } else {
                Operation::MarkedContentPoint(name_str(tag))
            }
        }
        PdfOp::XObject { name } => Operation::PaintXObject(name_str(name)),
        PdfOp::Shade { name } => Operation::Shade(name_str(name)),
        PdfOp::InlineImage { image } => {
            let dict: ir::PdfDict = vec![
                ("Width".into(), ir::PdfValue::Integer(image.width as i64)),
                ("Height".into(), ir::PdfValue::Integer(image.height as i64)),
                (
                    "BitsPerComponent".into(),
                    ir::PdfValue::Integer(image.bits_per_component.unwrap_or(8) as i64),
                ),
                (
                    "ColorSpace".into(),
                    ir::PdfValue::Name(
                        image
                            .color_space
                            .as_ref()
                            .map(color_space_name)
                            .unwrap_or_default(),
                    ),
                ),
            ];
            let data = image.image_data(resolver).unwrap_or_default().to_vec();
            Operation::InlineImage(dict, data)
        }
    }
}

fn to_ir_matrix(m: &PdfMatrix) -> ir::Matrix {
    ir::Matrix {
        a: m.a as f64,
        b: m.b as f64,
        c: m.c as f64,
        d: m.d as f64,
        e: m.e as f64,
        f: m.f as f64,
    }
}

fn color_vec(c: &Color) -> Vec<f64> {
    match c {
        Color::Gray(g) => vec![*g as f64],
        Color::Rgb(rgb) => vec![rgb.red as f64, rgb.green as f64, rgb.blue as f64],
        Color::Cmyk(cmyk) => vec![
            cmyk.cyan as f64,
            cmyk.magenta as f64,
            cmyk.yellow as f64,
            cmyk.key as f64,
        ],
        Color::Other(args) => args.iter().filter_map(primitive_to_f64).collect(),
    }
}

fn primitive_to_f64(p: &Primitive) -> Option<f64> {
    match p {
        Primitive::Integer(i) => Some(*i as f64),
        Primitive::Number(f) => Some(*f as f64),
        _ => None,
    }
}

fn color_space_name(cs: &PdfColorSpace) -> String {
    match cs {
        PdfColorSpace::DeviceGray => "DeviceGray",
        PdfColorSpace::DeviceRGB => "DeviceRGB",
        PdfColorSpace::DeviceCMYK => "DeviceCMYK",
        PdfColorSpace::CalGray(_) => "CalGray",
        PdfColorSpace::CalRGB(_) => "CalRGB",
        PdfColorSpace::CalCMYK(_) => "CalCMYK",
        PdfColorSpace::Indexed(..) => "Indexed",
        PdfColorSpace::Separation(..) => "Separation",
        PdfColorSpace::Icc(_) => "ICCBased",
        PdfColorSpace::Pattern => "Pattern",
        PdfColorSpace::Named(_) => "Named",
        PdfColorSpace::DeviceN { .. } => "DeviceN",
        PdfColorSpace::Other(_) => "Other",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal spec-valid one-page PDF (correct xref offsets).
    fn sample_pdf() -> Vec<u8> {
        let stream = "0.2 0.4 0.8 rg\n10 10 80 60 re\nf\n0 0 0 RG\n2 w\n10 10 m\n190 190 l\nS\nBT /F1 12 Tf 1 0 0 1 20 150 Tm (Hello) Tj ET\n";
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".into(),
            format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        ];
        let mut body = b"%PDF-1.4\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            let num = i + 1;
            offsets.push(body.len());
            body.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
            body.extend_from_slice(obj.as_bytes());
            body.extend_from_slice(b"\nendobj\n");
        }
        let xref_start = body.len();
        let mut xref = String::from("xref\n");
        xref.push_str(&format!("0 {}\n", objects.len() + 1));
        xref.push_str("0000000000 65535 f \n");
        for off in &offsets {
            xref.push_str(&format!("{off:010} 00000 n \n"));
        }
        body.extend_from_slice(xref.as_bytes());
        body.extend_from_slice(
            format!(
                "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{}\n%%EOF",
                objects.len() + 1,
                xref_start
            )
            .as_bytes(),
        );
        body
    }

    #[test]
    fn parses_pages_and_dimensions() {
        let doc = parse_bytes(&sample_pdf()).unwrap();
        assert_eq!(doc.page_count(), 1);
        let page = doc.page(0).unwrap();
        assert_eq!(page.width(), 200.0);
        assert_eq!(page.height(), 200.0);
    }

    #[test]
    fn content_stream_ops_are_converted() {
        let doc = parse_bytes(&sample_pdf()).unwrap();
        let page = doc.page(0).unwrap();
        assert!(!page.contents.is_empty());
        let ops = &page.contents[0].ops;
        assert!(ops.len() >= 8);
        assert!(ops.iter().any(|op| matches!(op, Operation::Rectangle(..))));
        assert!(ops.iter().any(|op| matches!(op, Operation::Fill)));
        assert!(ops.iter().any(|op| matches!(op, Operation::TextDraw(_))));
    }

    #[test]
    fn resources_are_converted() {
        let doc = parse_bytes(&sample_pdf()).unwrap();
        let page = doc.page(0).unwrap();
        assert_eq!(page.resources.fonts.len(), 1);
        assert_eq!(page.resources.fonts[0].0, "F1");
    }

    #[test]
    fn trailer_root_is_present() {
        let doc = parse_bytes(&sample_pdf()).unwrap();
        assert!(doc.trailer.root > 0);
    }

    #[test]
    fn indirect_objects_are_populated() {
        let doc = parse_bytes(&sample_pdf()).unwrap();
        // The sample has 5 objects (catalog, pages, page, contents, font).
        assert_eq!(doc.objects.len(), 5);
        assert!(doc.objects.iter().any(|&(num, gen, ref v)| {
            num == 4
                && gen == 0
                && matches!(v, ir::PdfValue::Stream(ref s) if !s.data.is_empty())
        }));
        assert!(doc.objects.iter().any(|&(num, _, ref v)| {
            num == 1 && matches!(v, ir::PdfValue::Dict(_))
        }));
    }

    #[test]
    fn xref_entries_have_real_offsets() {
        let data = sample_pdf();
        let doc = parse_bytes(&data).unwrap();
        // entries are indexed by object number (entry 0 = free head).
        assert!(!doc.xref.entries.is_empty());
        assert!(!doc.xref.entries[0].in_use);
        for (num, entry) in doc.xref.entries.iter().enumerate().skip(1) {
            if entry.in_use {
                let offset = entry.offset as usize;
                let header = &data[offset..];
                assert!(header.starts_with(format!("{num} ").as_bytes()));
            }
        }
    }

    #[test]
    fn trailer_info_and_encrypt_populated() {
        let stream = "0.2 0.4 0.8 rg\n";
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".into(),
            format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            "<< /Title (Report) /Author (TPT) >>".into(),
        ];
        let data = build_pdf(&objects, "<< /Root 1 0 R /Size 7 /Info 6 0 R >>");
        let doc = parse_bytes(&data).unwrap();
        assert_eq!(doc.trailer.info, Some(6));
        assert!(doc.trailer.encrypt.is_none());
    }

    #[test]
    fn annotations_and_labels_populated() {
        let stream = "0.2 0.4 0.8 rg\n";
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [ 0 << /S /r >> 3 << /S /D /P (p) >> ] >> >>"
                .into(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 5 0 R /Annots [7 0 R] >>"
                .into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 5 0 R >>".into(),
            format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            "<< /Subtype /Text /Rect [10 10 20 20] /Contents (hello) >>".into(),
        ];
        let data = build_pdf(&objects, "<< /Root 1 0 R /Size 8 >>");
        let doc = parse_bytes(&data).unwrap();
        assert_eq!(doc.page_count(), 2);
        assert_eq!(doc.page(0).unwrap().label.as_deref(), Some("i"));
        assert_eq!(doc.page(1).unwrap().label.as_deref(), Some("p1"));
        let annots = &doc.page(0).unwrap().annotations;
        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].subtype, "Text");
        assert_eq!(annots[0].contents.as_deref(), Some("hello"));
    }

    /// Serialize a body of objects plus a trailer into a classic-xref PDF.
    fn build_pdf(objects: &[String], trailer: &str) -> Vec<u8> {
        let mut body = b"%PDF-1.4\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            let num = i + 1;
            offsets.push(body.len());
            body.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
            body.extend_from_slice(obj.as_bytes());
            body.extend_from_slice(b"\nendobj\n");
        }
        let xref_start = body.len();
        let mut xref = String::from("xref\n");
        xref.push_str(&format!("0 {}\n", objects.len() + 1));
        xref.push_str("0000000000 65535 f \n");
        for off in &offsets {
            xref.push_str(&format!("{off:010} 00000 n \n"));
        }
        body.extend_from_slice(xref.as_bytes());
        body.extend_from_slice(
            format!("trailer\n{trailer}\nstartxref\n{}\n%%EOF", xref_start).as_bytes(),
        );
        body
    }
}
