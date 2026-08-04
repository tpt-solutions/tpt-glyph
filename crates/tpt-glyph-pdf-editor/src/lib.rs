// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-editor
//
// A transactional, functional-style PDF editing API: load a document into
// the canonical `tpt-glyph-pdf-ir` model, apply edits that each return a new
// `Editor` value (mirroring `GraphicsState`'s "operators produce a new
// value" design — see `docs/architecture.md`), and `save()` to serialize
// the result via `tpt-glyph-pdf-writer`.
//
// `save()` rebuilds the PDF from the semantic IR tree rather than patching
// the original byte-level object graph (see `build`'s module docs for why,
// and what that trades away).

//! # tpt-glyph-pdf-editor
//!
//! ```
//! use tpt_glyph_pdf_editor::Editor;
//! use tpt_glyph_pdf_ir::{ContentStream, Document, Operation, Page, Rect, Resources, Trailer, XRef};
//!
//! let doc = Document {
//!     version: "1.7".into(),
//!     pages: vec![Page {
//!         index: 0,
//!         label: None,
//!         media_box: Rect::new(0.0, 0.0, 200.0, 200.0),
//!         crop_box: None,
//!         bleed_box: None,
//!         trim_box: None,
//!         art_box: None,
//!         rotate: 0,
//!         resources: Resources::empty(),
//!         contents: vec![ContentStream::new(vec![
//!             Operation::BeginText,
//!             Operation::TextDraw(b"Hello".to_vec()),
//!             Operation::EndText,
//!         ])],
//!         annotations: Vec::new(),
//!         thumb: None,
//!         struct_parents: None,
//!     }],
//!     xref: XRef { entries: Vec::new() },
//!     trailer: Trailer { root: 1, info: None, id: None, encrypt: None },
//!     objects: Vec::new(),
//! };
//!
//! let edited = Editor::from_document(doc)
//!     .replace_text(0, b"Hello", b"Goodbye")
//!     .unwrap();
//! let bytes = edited.save().unwrap();
//! assert!(bytes.starts_with(b"%PDF-"));
//! ```

mod build;
mod content;

use tpt_glyph_pdf_ir::{Document, Matrix, Operation, TextSegment, XObject};

/// Errors produced while loading, editing, or saving a document.
#[derive(Debug)]
pub enum EditError {
    /// A page index was out of range for the document.
    PageNotFound { index: usize, page_count: usize },
    /// `replace_text` found no matching text run to replace.
    TextNotFound,
    /// Loading the source PDF failed.
    Parse(tpt_glyph_pdf_parser::ParseError),
    /// Serializing the edited document failed.
    Write(tpt_glyph_pdf_writer::WriteError),
}

impl core::fmt::Display for EditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EditError::PageNotFound { index, page_count } => {
                write!(
                    f,
                    "page {index} does not exist (document has {page_count} pages)"
                )
            }
            EditError::TextNotFound => write!(f, "no matching text run found to replace"),
            EditError::Parse(e) => write!(f, "failed to parse source PDF: {e}"),
            EditError::Write(e) => write!(f, "failed to serialize edited PDF: {e}"),
        }
    }
}

impl std::error::Error for EditError {}

impl From<tpt_glyph_pdf_parser::ParseError> for EditError {
    fn from(e: tpt_glyph_pdf_parser::ParseError) -> Self {
        EditError::Parse(e)
    }
}

impl From<tpt_glyph_pdf_writer::WriteError> for EditError {
    fn from(e: tpt_glyph_pdf_writer::WriteError) -> Self {
        EditError::Write(e)
    }
}

pub type Result<T> = core::result::Result<T, EditError>;

/// A raw, already-decoded image to embed via [`Editor::insert_image`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    /// A PDF color space name (e.g. `"DeviceRGB"`, `"DeviceGray"`).
    pub color_space: String,
    /// Raw (unencoded) sample data; `tpt-glyph-pdf-writer` Flate-compresses
    /// it on save.
    pub data: Vec<u8>,
}

/// A transactional PDF editor: each edit method consumes `self` and returns
/// a new `Editor` wrapping a new, immutable [`Document`] — the original is
/// never mutated in place.
#[derive(Debug, Clone)]
pub struct Editor {
    doc: Document,
}

impl Editor {
    /// Load and parse a PDF from a file path.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(Self::from_document(tpt_glyph_pdf_parser::parse_path(path)?))
    }

    /// Load and parse a PDF from an in-memory buffer.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Ok(Self::from_document(tpt_glyph_pdf_parser::parse_bytes(
            data,
        )?))
    }

    /// Wrap an already-parsed [`Document`] for editing.
    pub fn from_document(doc: Document) -> Self {
        Self { doc }
    }

    /// Borrow the current document.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Consume the editor, returning its current document.
    pub fn into_document(self) -> Document {
        self.doc
    }

    /// Replace every occurrence of `needle` with `replacement` in page
    /// `page_index`'s text-showing operators (`Tj`/`TJ`/`'`/`"`), matching
    /// on the raw shown bytes.
    ///
    /// This is a literal byte substitution: it does not re-measure or
    /// re-flow surrounding text, so a replacement of substantially
    /// different width may visually overlap or leave a gap. Returns
    /// [`EditError::TextNotFound`] if `needle` doesn't occur anywhere on the
    /// page.
    pub fn replace_text(
        mut self,
        page_index: usize,
        needle: &[u8],
        replacement: &[u8],
    ) -> Result<Self> {
        let page_count = self.doc.pages.len();
        let page = self
            .doc
            .pages
            .get_mut(page_index)
            .ok_or(EditError::PageNotFound {
                index: page_index,
                page_count,
            })?;

        let mut replaced = false;
        for stream in &mut page.contents {
            for op in &mut stream.ops {
                match op {
                    Operation::TextDraw(bytes) | Operation::TextNewlineAndDraw(bytes) => {
                        if bytes == needle {
                            *bytes = replacement.to_vec();
                            replaced = true;
                        }
                    }
                    Operation::TextNewlineWithSpacingAndDraw(_, _, bytes) => {
                        if bytes == needle {
                            *bytes = replacement.to_vec();
                            replaced = true;
                        }
                    }
                    Operation::TextDrawAdjusted(segments) => {
                        for seg in segments {
                            if let TextSegment::Text(bytes) = seg {
                                if bytes == needle {
                                    *bytes = replacement.to_vec();
                                    replaced = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if replaced {
            Ok(self)
        } else {
            Err(EditError::TextNotFound)
        }
    }

    /// Insert `image` on page `page_index`, scaled to fill `rect` (page
    /// user-space units).
    ///
    /// A fresh `/Resources /XObject` entry is added and a `q ... Do Q`
    /// fragment is appended to the page's content stream.
    pub fn insert_image(
        mut self,
        page_index: usize,
        image: ImageData,
        rect: tpt_glyph_pdf_ir::Rect,
    ) -> Result<Self> {
        let page_count = self.doc.pages.len();
        let page = self
            .doc
            .pages
            .get_mut(page_index)
            .ok_or(EditError::PageNotFound {
                index: page_index,
                page_count,
            })?;

        let name = fresh_xobject_name(&page.resources.xobjects);
        page.resources.xobjects.push((
            name.clone(),
            XObject::Image {
                width: image.width,
                height: image.height,
                bits_per_component: image.bits_per_component,
                color_space: image.color_space,
                data: image.data,
                mask: None,
                smask: None,
            },
        ));

        let placement = vec![
            Operation::Save,
            Operation::ConcatMatrix(Matrix::new(
                rect.width(),
                0.0,
                0.0,
                rect.height(),
                rect.left,
                rect.bottom,
            )),
            Operation::PaintXObject(name),
            Operation::Restore,
        ];
        match page.contents.last_mut() {
            Some(stream) => stream.ops.extend(placement),
            None => page
                .contents
                .push(tpt_glyph_pdf_ir::ContentStream::new(placement)),
        }

        Ok(self)
    }

    /// Serialize the current document into a complete PDF file's bytes.
    ///
    /// This rebuilds the file from the semantic IR (see the `build` module
    /// docs), so only objects reachable from `Document.pages` are ever
    /// written — an unreferenced resource never makes it into the output.
    pub fn save(&self) -> Result<Vec<u8>> {
        Ok(build::build(&self.doc)?)
    }
}

fn fresh_xobject_name(existing: &[(String, XObject)]) -> String {
    let mut n = 0u32;
    loop {
        let candidate = format!("Im{n}");
        if !existing.iter().any(|(name, _)| name == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_glyph_pdf_ir::{ContentStream, Page, Rect, Resources, Trailer, XRef};

    fn one_page_doc(ops: Vec<Operation>) -> Document {
        Document {
            version: "1.7".into(),
            pages: vec![Page {
                index: 0,
                label: None,
                media_box: Rect::new(0.0, 0.0, 200.0, 200.0),
                crop_box: None,
                bleed_box: None,
                trim_box: None,
                art_box: None,
                rotate: 0,
                resources: Resources::empty(),
                contents: vec![ContentStream::new(ops)],
                annotations: Vec::new(),
                thumb: None,
                struct_parents: None,
            }],
            xref: XRef {
                entries: Vec::new(),
            },
            trailer: Trailer {
                root: 1,
                info: None,
                id: None,
                encrypt: None,
            },
            objects: Vec::new(),
        }
    }

    #[test]
    fn replace_text_updates_matching_tj() {
        let doc = one_page_doc(vec![
            Operation::BeginText,
            Operation::TextDraw(b"Hello".to_vec()),
            Operation::EndText,
        ]);
        let edited = Editor::from_document(doc)
            .replace_text(0, b"Hello", b"Goodbye")
            .unwrap();
        let ops = &edited.document().pages[0].contents[0].ops;
        assert!(ops.contains(&Operation::TextDraw(b"Goodbye".to_vec())));
    }

    #[test]
    fn replace_text_updates_matching_segment_in_tj_array() {
        let doc = one_page_doc(vec![Operation::TextDrawAdjusted(vec![
            TextSegment::Text(b"AB".to_vec()),
            TextSegment::Spacing(-50.0),
            TextSegment::Text(b"CD".to_vec()),
        ])]);
        let edited = Editor::from_document(doc)
            .replace_text(0, b"CD", b"EF")
            .unwrap();
        let Operation::TextDrawAdjusted(segments) = &edited.document().pages[0].contents[0].ops[0]
        else {
            panic!("expected TextDrawAdjusted");
        };
        assert_eq!(segments[2], TextSegment::Text(b"EF".to_vec()));
    }

    #[test]
    fn replace_text_missing_needle_errors() {
        let doc = one_page_doc(vec![Operation::TextDraw(b"Hello".to_vec())]);
        let err = Editor::from_document(doc)
            .replace_text(0, b"Nope", b"X")
            .unwrap_err();
        assert!(matches!(err, EditError::TextNotFound));
    }

    #[test]
    fn replace_text_out_of_range_page_errors() {
        let doc = one_page_doc(vec![]);
        let err = Editor::from_document(doc)
            .replace_text(5, b"x", b"y")
            .unwrap_err();
        assert!(matches!(
            err,
            EditError::PageNotFound {
                index: 5,
                page_count: 1
            }
        ));
    }

    #[test]
    fn insert_image_adds_xobject_and_paints_it() {
        let doc = one_page_doc(vec![]);
        let image = ImageData {
            width: 2,
            height: 2,
            bits_per_component: 8,
            color_space: "DeviceRGB".into(),
            data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
        };
        let edited = Editor::from_document(doc)
            .insert_image(0, image, Rect::new(10.0, 10.0, 60.0, 60.0))
            .unwrap();
        let page = &edited.document().pages[0];
        assert_eq!(page.resources.xobjects.len(), 1);
        assert!(page.contents[0]
            .ops
            .iter()
            .any(|op| matches!(op, Operation::PaintXObject(_))));
    }

    #[test]
    fn save_produces_a_well_formed_pdf() {
        let doc = one_page_doc(vec![
            Operation::FillRgb(0.2, 0.4, 0.8),
            Operation::Rectangle(10.0, 10.0, 80.0, 60.0),
            Operation::Fill,
        ]);
        let bytes = Editor::from_document(doc).save().unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn end_to_end_load_edit_save_round_trips_through_the_parser() {
        // A hand-built minimal PDF with one text-showing operator.
        let stream = "BT /F1 12 Tf (Hello) Tj ET";
        let objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".into(),
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            ),
        ];
        let mut body = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(body.len());
            body.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            body.extend_from_slice(obj.as_bytes());
            body.extend_from_slice(b"\nendobj\n");
        }
        let xref_start = body.len();
        let mut xref = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for off in &offsets {
            xref.push_str(&format!("{off:010} 00000 n \n"));
        }
        body.extend_from_slice(xref.as_bytes());
        body.extend_from_slice(
            format!(
                "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref_start}\n%%EOF",
                objects.len() + 1
            )
            .as_bytes(),
        );

        let edited = Editor::from_bytes(&body)
            .unwrap()
            .replace_text(0, b"Hello", b"Goodbye")
            .unwrap();
        let saved = edited.save().unwrap();

        // Re-parse the saved output through the same parser to confirm it's
        // valid AND that the edit actually took effect.
        let reparsed = tpt_glyph_pdf_parser::parse_bytes(&saved).unwrap();
        assert_eq!(reparsed.page_count(), 1);
        let ops = &reparsed.page(0).unwrap().contents[0].ops;
        assert!(ops.contains(&Operation::TextDraw(b"Goodbye".to_vec())));
    }
}
