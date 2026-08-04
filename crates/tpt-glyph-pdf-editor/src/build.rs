// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-editor / build
//
// Rebuilds a complete PDF from the semantic `tpt-glyph-pdf-ir::Document` tree
// via `tpt-glyph-pdf-writer`. Because this is a *rebuild from the semantic
// model* rather than a patch of the original byte-level object graph, object
// numbers are freely reassigned — which also means "garbage collection of
// unused objects" is inherent rather than a separate pass: nothing gets
// written unless it's reachable from `Document.pages`, by construction.
//
// The tradeoff: fields that carry a raw `(object number, generation)`
// reference into the *original* document (`FontRef::to_unicode`,
// `ExtGState::soft_mask`/`transfer`/`halftone`, `Resources::patterns`/
// `shadings`/`properties`, `XObject::Reference`, ...) can't be reproduced —
// that object number means nothing once every object is renumbered — so
// they're dropped on save rather than emitted as a dangling/incorrect
// reference. This is a deliberate, documented v1 limitation, not an
// oversight: silently emitting a wrong reference would be worse than
// omitting the value.

use crate::content::write_operations;
use tpt_glyph_pdf_ir::{Document, ExtGState, FontRef, Page, PdfValue, Resources, XObject};
use tpt_glyph_pdf_writer::{ObjectId, Stream, Value, WriteOptions, Writer};

/// Rebuild `doc` into a complete PDF file's bytes.
pub fn build(doc: &Document) -> tpt_glyph_pdf_writer::Result<Vec<u8>> {
    let mut w = Writer::with_options(WriteOptions {
        header_version: header_version(doc),
        ..WriteOptions::default()
    });

    let pages_id = w.alloc();
    let mut kids = Vec::with_capacity(doc.pages.len());
    for page in &doc.pages {
        kids.push(Value::reference(write_page(&mut w, page, pages_id)));
    }
    w.define(
        pages_id,
        Value::dict([
            ("Type", Value::name("Pages")),
            ("Kids", Value::Array(kids)),
            ("Count", Value::Integer(doc.pages.len() as i64)),
        ]),
    )
    .expect("pages_id was just allocated");

    let catalog_id = w.add(Value::dict([
        ("Type", Value::name("Catalog")),
        ("Pages", Value::reference(pages_id)),
    ]));
    w.set_root(catalog_id);

    w.finish()
}

/// `%PDF-x.y` must be a `'1'.'0'..'9'` two-character version; fall back to
/// 1.7 if the source document's version string doesn't parse as one.
fn header_version(doc: &Document) -> &'static str {
    const KNOWN: &[&str] = &[
        "1.0", "1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "1.7", "2.0",
    ];
    KNOWN
        .iter()
        .find(|&&v| v == doc.version)
        .copied()
        .unwrap_or("1.7")
}

fn write_page(w: &mut Writer, page: &Page, parent: ObjectId) -> ObjectId {
    let resources = write_resources(w, &page.resources);

    let mut all_ops = Vec::new();
    for stream in &page.contents {
        all_ops.extend(stream.ops.iter().cloned());
    }
    let mut content_bytes = Vec::new();
    write_operations(&all_ops, &mut content_bytes);
    let mut content_stream = Stream::new(content_bytes);
    content_stream.compress();
    let contents_id = w.add_stream(content_stream);

    let mut dict = vec![
        ("Type".to_string(), Value::name("Page")),
        ("Parent".to_string(), Value::reference(parent)),
        ("MediaBox".to_string(), rect_value(&page.media_box)),
        ("Resources".to_string(), resources),
        ("Contents".to_string(), Value::reference(contents_id)),
    ];
    if let Some(crop) = &page.crop_box {
        dict.push(("CropBox".to_string(), rect_value(crop)));
    }
    if page.rotate != 0 {
        dict.push(("Rotate".to_string(), Value::Integer(page.rotate as i64)));
    }
    if !page.annotations.is_empty() {
        let annots = page
            .annotations
            .iter()
            .map(|a| Value::reference(w.add(pdfvalue_to_value(&a.value))))
            .collect();
        dict.push(("Annots".to_string(), Value::Array(annots)));
    }

    w.add(Value::Dict(dict))
}

fn rect_value(r: &tpt_glyph_pdf_ir::Rect) -> Value {
    Value::array([
        Value::Real(r.left),
        Value::Real(r.bottom),
        Value::Real(r.right),
        Value::Real(r.top),
    ])
}

fn write_resources(w: &mut Writer, res: &Resources) -> Value {
    let mut dict: Vec<(String, Value)> = Vec::new();

    if !res.fonts.is_empty() {
        let entries = res
            .fonts
            .iter()
            .map(|(name, font)| (name.clone(), Value::reference(write_font(w, font))))
            .collect();
        dict.push(("Font".to_string(), Value::Dict(entries)));
    }

    if !res.xobjects.is_empty() {
        let entries: Vec<(String, Value)> = res
            .xobjects
            .iter()
            .filter_map(|(name, xobj)| {
                write_xobject(w, xobj).map(|id| (name.clone(), Value::reference(id)))
            })
            .collect();
        if !entries.is_empty() {
            dict.push(("XObject".to_string(), Value::Dict(entries)));
        }
    }

    if !res.ext_gstates.is_empty() {
        let entries = res
            .ext_gstates
            .iter()
            .map(|(name, gs)| (name.clone(), ext_gstate_value(gs)))
            .collect();
        dict.push(("ExtGState".to_string(), Value::Dict(entries)));
    }

    if !res.color_spaces.is_empty() {
        let entries = res
            .color_spaces
            .iter()
            .map(|(name, v)| (name.clone(), pdfvalue_to_value(v)))
            .collect();
        dict.push(("ColorSpace".to_string(), Value::Dict(entries)));
    }

    Value::Dict(dict)
}

fn write_font(w: &mut Writer, font: &FontRef) -> ObjectId {
    let mut dict = vec![
        ("Type".to_string(), Value::name("Font")),
        ("Subtype".to_string(), Value::name(font.subtype.clone())),
        ("BaseFont".to_string(), Value::name(font.base_font.clone())),
    ];
    if !font.widths.is_empty() {
        dict.push((
            "FirstChar".to_string(),
            Value::Integer(font.first_char as i64),
        ));
        dict.push((
            "LastChar".to_string(),
            Value::Integer(font.last_char as i64),
        ));
        dict.push((
            "Widths".to_string(),
            Value::array(font.widths.iter().map(|w| Value::Real(*w))),
        ));
    }
    if let Some(encoding) = &font.encoding {
        dict.push(("Encoding".to_string(), Value::name(encoding.clone())));
    }
    w.add(Value::Dict(dict))
}

fn write_xobject(w: &mut Writer, xobj: &XObject) -> Option<ObjectId> {
    match xobj {
        XObject::Image {
            width,
            height,
            bits_per_component,
            color_space,
            data,
            ..
        } => {
            let dict = vec![
                ("Type".to_string(), Value::name("XObject")),
                ("Subtype".to_string(), Value::name("Image")),
                ("Width".to_string(), Value::Integer(*width as i64)),
                ("Height".to_string(), Value::Integer(*height as i64)),
                (
                    "BitsPerComponent".to_string(),
                    Value::Integer(*bits_per_component as i64),
                ),
                (
                    "ColorSpace".to_string(),
                    Value::name(if color_space.is_empty() {
                        "DeviceGray".to_string()
                    } else {
                        color_space.clone()
                    }),
                ),
            ];
            let mut stream = Stream::with_dict(dict, data.clone());
            stream.compress();
            Some(w.add_stream(stream))
        }
        XObject::Form {
            bbox,
            matrix,
            resources,
            ops,
        } => {
            let resources_value = write_resources(w, resources);
            let mut bytes = Vec::new();
            write_operations(ops, &mut bytes);
            let mut stream = Stream::new(bytes);
            stream.compress();
            let dict = vec![
                ("Type".to_string(), Value::name("XObject")),
                ("Subtype".to_string(), Value::name("Form")),
                ("BBox".to_string(), rect_value(bbox)),
                (
                    "Matrix".to_string(),
                    Value::array([
                        Value::Real(matrix.a),
                        Value::Real(matrix.b),
                        Value::Real(matrix.c),
                        Value::Real(matrix.d),
                        Value::Real(matrix.e),
                        Value::Real(matrix.f),
                    ]),
                ),
                ("Resources".to_string(), resources_value),
            ];
            stream.dict.extend(dict);
            Some(w.add_stream(stream))
        }
        // An XObject the original parse couldn't resolve into Form/Image
        // data; there's nothing to re-embed, so this resource is dropped
        // (see the module-level docs).
        XObject::Reference(_) => None,
    }
}

fn ext_gstate_value(gs: &ExtGState) -> Value {
    let mut dict: Vec<(String, Value)> = vec![("Type".to_string(), Value::name("ExtGState"))];
    if let Some(w) = gs.line_width {
        dict.push(("LW".to_string(), Value::Real(w)));
    }
    if let Some(c) = gs.line_cap {
        dict.push(("LC".to_string(), Value::Integer(c as i64)));
    }
    if let Some(j) = gs.line_join {
        dict.push(("LJ".to_string(), Value::Integer(j as i64)));
    }
    if let Some(m) = gs.miter_limit {
        dict.push(("ML".to_string(), Value::Real(m)));
    }
    if let Some((pattern, phase)) = &gs.dash_pattern {
        dict.push((
            "D".to_string(),
            Value::array([
                Value::array(pattern.iter().map(|p| Value::Real(*p))),
                Value::Integer(*phase as i64),
            ]),
        ));
    }
    if let Some(ri) = &gs.rendering_intent {
        dict.push(("RI".to_string(), Value::name(ri.clone())));
    }
    if let Some(ca) = gs.alpha_stroke {
        dict.push(("CA".to_string(), Value::Real(ca)));
    }
    if let Some(ca) = gs.alpha_fill {
        dict.push(("ca".to_string(), Value::Real(ca)));
    }
    Value::Dict(dict)
}

/// Convert an IR `PdfValue` into a writer `Value`. `Reference`s from the
/// source document can't be reproduced against the rebuilt object numbering
/// (see the module docs), so they become `Null`.
fn pdfvalue_to_value(v: &PdfValue) -> Value {
    match v {
        PdfValue::Null => Value::Null,
        PdfValue::Boolean(b) => Value::Boolean(*b),
        PdfValue::Integer(i) => Value::Integer(*i),
        PdfValue::Real(f) => Value::Real(*f),
        PdfValue::Name(n) => Value::name(n.clone()),
        PdfValue::String(s) => Value::string(s.clone()),
        PdfValue::HexString(s) => Value::HexString(s.clone()),
        PdfValue::Array(items) => Value::Array(items.iter().map(pdfvalue_to_value).collect()),
        PdfValue::Dict(entries) => Value::Dict(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), pdfvalue_to_value(v)))
                .collect(),
        ),
        PdfValue::Stream(_) | PdfValue::Reference(..) => Value::Null,
    }
}
