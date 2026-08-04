// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — out-glyph-diag / lint
//
// Structural sanity checks over a parsed `tpt-glyph-pdf-ir::Document`, used
// by the `check` subcommand to flag corrupted or non-standard PDF structure
// (Phase 14). Parsing itself already rejects unreadable files; these checks
// catch documents that parse but look structurally suspicious — the kind of
// thing that tends to produce a blank page or a silent misrender rather
// than an outright parse error.

use tpt_glyph_pdf_ir::{Document, Operation};

/// A single structural finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub page: Option<usize>,
    pub message: String,
}

/// Run every structural check against `doc`, returning every finding.
pub fn lint(doc: &Document) -> Vec<Finding> {
    let mut findings = Vec::new();

    if doc.pages.is_empty() {
        findings.push(Finding {
            page: None,
            message: "document has zero pages".to_string(),
        });
    }

    if doc.trailer.root == 0 {
        findings.push(Finding {
            page: None,
            message: "trailer /Root references object 0, which is never a valid indirect object"
                .to_string(),
        });
    }

    for (num, entry) in doc.xref.entries.iter().enumerate() {
        // Entry 0 is the mandatory free-list head; a real in-use object
        // resolving to byte offset 0 means the best-effort `N G obj` scan
        // (see `tpt-glyph-pdf-parser::compute_object_offsets`) couldn't find
        // it — typical of incremental-update files (a later revision
        // shadows an earlier `obj` header at the same number) or documents
        // using cross-reference streams instead of a classic `xref` table.
        if num != 0 && entry.in_use && entry.offset == 0 {
            findings.push(Finding {
                page: None,
                message: format!(
                    "object {num}'s byte offset could not be located (likely an incremental \
                     update or a cross-reference-stream document; the object inventory may be \
                     incomplete)"
                ),
            });
        }
    }

    for page in &doc.pages {
        if page.media_box.width() <= 0.0 || page.media_box.height() <= 0.0 {
            findings.push(Finding {
                page: Some(page.index as usize),
                message: format!(
                    "degenerate MediaBox ({}x{})",
                    page.media_box.width(),
                    page.media_box.height()
                ),
            });
        }

        for stream in &page.contents {
            for op in &stream.ops {
                match op {
                    Operation::TextFont(name, _) => {
                        if !page.resources.fonts.iter().any(|(n, _)| n == name) {
                            findings.push(Finding {
                                page: Some(page.index as usize),
                                message: format!(
                                    "content stream references undefined font resource /{name}"
                                ),
                            });
                        }
                    }
                    Operation::PaintXObject(name) => {
                        if !page.resources.xobjects.iter().any(|(n, _)| n == name) {
                            findings.push(Finding {
                                page: Some(page.index as usize),
                                message: format!(
                                    "content stream references undefined XObject resource /{name}"
                                ),
                            });
                        }
                    }
                    Operation::SetGraphicsState(name)
                        if !page.resources.ext_gstates.iter().any(|(n, _)| n == name) =>
                    {
                        findings.push(Finding {
                            page: Some(page.index as usize),
                            message: format!(
                                "content stream references undefined ExtGState resource /{name}"
                            ),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_glyph_pdf_ir::{ContentStream, Page, Rect, Resources, Trailer, XRef, XRefEntry};

    fn base_page(ops: Vec<Operation>) -> Page {
        Page {
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
        }
    }

    fn base_doc(pages: Vec<Page>) -> Document {
        Document {
            version: "1.7".into(),
            pages,
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
    fn clean_document_has_no_findings() {
        let doc = base_doc(vec![base_page(vec![])]);
        assert!(lint(&doc).is_empty());
    }

    #[test]
    fn zero_pages_is_flagged() {
        let doc = base_doc(vec![]);
        let findings = lint(&doc);
        assert!(findings.iter().any(|f| f.message.contains("zero pages")));
    }

    #[test]
    fn invalid_root_is_flagged() {
        let mut doc = base_doc(vec![base_page(vec![])]);
        doc.trailer.root = 0;
        let findings = lint(&doc);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("/Root references object 0")));
    }

    #[test]
    fn degenerate_media_box_is_flagged() {
        let mut page = base_page(vec![]);
        page.media_box = Rect::new(0.0, 0.0, 0.0, 200.0);
        let doc = base_doc(vec![page]);
        let findings = lint(&doc);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("degenerate MediaBox")));
    }

    #[test]
    fn unresolved_xref_offset_is_flagged() {
        let mut doc = base_doc(vec![base_page(vec![])]);
        doc.xref.entries = vec![
            XRefEntry::new(0, 65535, false),
            XRefEntry::new(0, 0, true), // object 1: in_use but offset never located
        ];
        let findings = lint(&doc);
        assert!(findings.iter().any(|f| f
            .message
            .contains("object 1's byte offset could not be located")));
    }

    #[test]
    fn undefined_font_resource_is_flagged() {
        let doc = base_doc(vec![base_page(vec![Operation::TextFont(
            "F1".into(),
            12.0,
        )])]);
        let findings = lint(&doc);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("undefined font resource /F1")));
    }

    #[test]
    fn undefined_xobject_resource_is_flagged() {
        let doc = base_doc(vec![base_page(vec![Operation::PaintXObject("Im0".into())])]);
        let findings = lint(&doc);
        assert!(findings
            .iter()
            .any(|f| f.message.contains("undefined XObject resource /Im0")));
    }
}
