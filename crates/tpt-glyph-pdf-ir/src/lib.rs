// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-ir
//
// Canonical immutable intermediate representation of a PDF document. All
// transformations on the IR produce new values (functional style), making
// concurrent access safe by construction.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// A rectangle in PDF user-space units.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rect {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl Rect {
    pub const fn new(left: f64, bottom: f64, right: f64, top: f64) -> Self {
        Self {
            left,
            bottom,
            right,
            top,
        }
    }

    pub fn width(&self) -> f64 {
        (self.right - self.left).abs()
    }

    pub fn height(&self) -> f64 {
        (self.top - self.bottom).abs()
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
}

/// An affine transformation matrix in PDF notation `[a b c d e f]`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
}

// ---------------------------------------------------------------------------
// XRef (Cross-Reference)
// ---------------------------------------------------------------------------

/// An entry in the cross-reference table.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct XRefEntry {
    pub offset: u64,
    pub generation: u16,
    pub in_use: bool,
}

impl XRefEntry {
    pub const fn new(offset: u64, generation: u16, in_use: bool) -> Self {
        Self {
            offset,
            generation,
            in_use,
        }
    }
}

/// Cross-reference table: maps object numbers to their byte offset.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct XRef {
    pub entries: Vec<XRefEntry>,
}

// ---------------------------------------------------------------------------
// Low-level PDF Objects
// ---------------------------------------------------------------------------

/// The raw value of a PDF object (pre-resolved).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PdfValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    String(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<PdfValue>),
    Dict(PdfDict),
    Stream(PdfStream),
    Reference(u32, u16), // object number, generation
}

/// A PDF dictionary: an ordered sequence of key-value pairs.
pub type PdfDict = Vec<(String, PdfValue)>;

/// A PDF stream: a dictionary followed by raw binary data.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PdfStream {
    pub dict: PdfDict,
    pub data: Vec<u8>,
}

impl PdfStream {
    pub const fn new(dict: PdfDict, data: Vec<u8>) -> Self {
        Self { dict, data }
    }
}

// ---------------------------------------------------------------------------
// Content Stream Operations
// ---------------------------------------------------------------------------

/// A single operation in a PDF content stream.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Operation {
    /// Save graphics state (q).
    Save,
    /// Restore graphics state (Q).
    Restore,
    /// Concatenate matrix to CTM (cm).
    ConcatMatrix(Matrix),
    /// Set stroking color space (CS).
    StrokeColorSpace(String),
    /// Set non-stroking color space (cs).
    FillColorSpace(String),
    /// Set stroking color (SC / SCN).
    StrokeColor(Vec<f64>),
    /// Set non-stroking color (sc / scn).
    FillColor(Vec<f64>),
    /// Set stroking gray (G).
    StrokeGray(f64),
    /// Set non-stroking gray (g).
    FillGray(f64),
    /// Set stroking RGB (RG).
    StrokeRgb(f64, f64, f64),
    /// Set non-stroking RGB (rg).
    FillRgb(f64, f64, f64),
    /// Set stroking CMYK (K).
    StrokeCmyk(f64, f64, f64, f64),
    /// Set non-stroking CMYK (k).
    FillCmyk(f64, f64, f64, f64),
    /// Set line width (w).
    LineWidth(f64),
    /// Set line cap (J).
    LineCap(u8),
    /// Set line join (j).
    LineJoin(u8),
    /// Set miter limit (M).
    MiterLimit(f64),
    /// Set dash pattern (d).
    DashPattern(Vec<f64>, u64),
    /// Set rendering intent (ri).
    RenderingIntent(String),
    /// Set flatness tolerance (i).
    Flatness(f64),
    /// Set graphics state parameters from a named entry (gs).
    SetGraphicsState(String),
    /// Begin a new subpath at (x, y) (m).
    MoveTo(f64, f64),
    /// Append a straight line to (x, y) (l).
    LineTo(f64, f64),
    /// Append a cubic Bézier curve (c).
    CurveTo(f64, f64, f64, f64, f64, f64),
    /// Append a cubic Bézier curve with coincident start control (v).
    CurveToV(f64, f64, f64, f64),
    /// Append a cubic Bézier curve with coincident end control (y).
    CurveToY(f64, f64, f64, f64),
    /// Append a rectangle (re).
    Rectangle(f64, f64, f64, f64),
    /// Close the current subpath (h).
    CloseSubpath,
    /// End the path without fill or stroke (n).
    EndPath,
    /// Stroke the path (S).
    Stroke,
    /// Close and stroke the path (s).
    CloseAndStroke,
    /// Fill the path (non-zero winding) (f / F).
    Fill,
    /// Fill the path (even-odd) (f*).
    FillEvenOdd,
    /// Fill and stroke (B).
    FillAndStroke,
    /// Fill and stroke (even-odd) (B*).
    FillAndStrokeEvenOdd,
    /// Close, fill, and stroke (b).
    CloseFillAndStroke,
    /// Close, fill (even-odd), and stroke (b*).
    CloseFillAndStrokeEvenOdd,
    /// Intersect with clip path (non-zero) (W).
    Clip,
    /// Intersect with clip path (even-odd) (W*).
    ClipEvenOdd,
    /// Begin text object (BT).
    BeginText,
    /// End text object (ET).
    EndText,
    /// Set character spacing (Tc).
    CharSpacing(f64),
    /// Set word spacing (Tw).
    WordSpacing(f64),
    /// Set horizontal text scaling (Tz).
    TextScaling(f64),
    /// Set leading (TL).
    Leading(f64),
    /// Set font and size (Tf).
    TextFont(String, f64),
    /// Set text rendering mode (Tr).
    TextRenderMode(u8),
    /// Set text rise (Ts).
    TextRise(f64),
    /// Move text position by offset (Td).
    MoveTextPosition(f64, f64),
    /// Move to next text line with leading offset (TD).
    MoveTextPositionAndLeading(f64, f64),
    /// Set text matrix (Tm).
    SetTextMatrix(Matrix),
    /// Move to the next line (T*).
    TextNewline,
    /// Show a text string (Tj).
    TextDraw(Vec<u8>),
    /// Show text with individual glyph positioning (TJ).
    TextDrawAdjusted(Vec<TextSegment>),
    /// Move to the start of the next line and show text (').
    TextNewlineAndDraw(Vec<u8>),
    /// Move to start of next line, set word/char spacing, and show text (").
    TextNewlineWithSpacingAndDraw(f64, f64, Vec<u8>),
    /// Begin marked-content sequence with tag (BMC).
    BeginMarkedContent(String),
    /// Begin marked-content sequence with property list (BDC).
    BeginMarkedContentWithProps(String),
    /// End marked-content sequence (EMC).
    EndMarkedContent,
    /// Marked-content point (MP).
    MarkedContentPoint(String),
    /// Marked-content point with property list (DP).
    MarkedContentPointWithProps(String),
    /// Paint XObject (Do).
    PaintXObject(String),
    /// Paint a shading (sh).
    Shade(String),
    /// Inline image (BI ... ID ... EI).
    InlineImage(PdfDict, Vec<u8>),
    /// Type 3 font glyph description (d0 / d1).
    SetCharWidth(f64, f64),
    SetCacheDevice(f64, f64, f64, f64, f64, f64, f64, f64),
}

/// A segment within a TJ (text draw adjusted) array.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TextSegment {
    Text(Vec<u8>),
    Spacing(f64),
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// A reference to a font resource.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FontRef {
    pub subtype: String,
    pub base_font: String,
    pub first_char: u32,
    pub last_char: u32,
    pub widths: Vec<f64>,
    pub descriptor: Option<PdfValue>,
    pub to_unicode: Option<PdfValue>,
    pub encoding: Option<String>,
}

/// An external object (XObject) reference.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum XObject {
    Form {
        bbox: Rect,
        matrix: Matrix,
        resources: Resources,
        ops: Vec<Operation>,
    },
    Image {
        width: u32,
        height: u32,
        bits_per_component: u8,
        color_space: String,
        data: Vec<u8>,
        mask: Option<Vec<u8>>,
        smask: Option<PdfValue>,
    },
    Reference(PdfValue),
}

/// Extended graphics state parameters.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExtGState {
    pub line_width: Option<f64>,
    pub line_cap: Option<u8>,
    pub line_join: Option<u8>,
    pub miter_limit: Option<f64>,
    pub dash_pattern: Option<(Vec<f64>, u64)>,
    pub rendering_intent: Option<String>,
    pub alpha_stroke: Option<f64>,
    pub alpha_fill: Option<f64>,
    pub blend_mode: Option<String>,
    pub soft_mask: Option<PdfValue>,
    pub stroke_adjust: Option<bool>,
    pub overprint_stroke: Option<bool>,
    pub overprint_fill: Option<bool>,
    pub overprint_mode: Option<u8>,
    pub transfer: Option<PdfValue>,
    pub halftone: Option<PdfValue>,
    pub flatness: Option<f64>,
    pub smoothness: Option<f64>,
}

/// Per-page resources: fonts, XObjects, and graphics states.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Resources {
    pub fonts: Vec<(String, FontRef)>,
    pub xobjects: Vec<(String, XObject)>,
    pub ext_gstates: Vec<(String, ExtGState)>,
    pub color_spaces: Vec<(String, PdfValue)>,
    pub patterns: Vec<(String, PdfValue)>,
    pub shadings: Vec<(String, PdfValue)>,
    pub properties: Vec<(String, PdfValue)>,
}

impl Resources {
    pub fn empty() -> Self {
        Self {
            fonts: Vec::new(),
            xobjects: Vec::new(),
            ext_gstates: Vec::new(),
            color_spaces: Vec::new(),
            patterns: Vec::new(),
            shadings: Vec::new(),
            properties: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Page Tree
// ---------------------------------------------------------------------------

/// An annotation on a page.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Annotation {
    pub subtype: String,
    pub rect: Rect,
    pub contents: Option<String>,
    pub value: PdfValue,
}

/// A single page in the document.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Page {
    pub index: u32,
    pub label: Option<String>,
    pub media_box: Rect,
    pub crop_box: Option<Rect>,
    pub bleed_box: Option<Rect>,
    pub trim_box: Option<Rect>,
    pub art_box: Option<Rect>,
    pub rotate: i32,
    pub resources: Resources,
    pub contents: Vec<ContentStream>,
    pub annotations: Vec<Annotation>,
    pub thumb: Option<PdfValue>,
    pub struct_parents: Option<u32>,
}

impl Page {
    pub fn width(&self) -> f64 {
        self.media_box.width()
    }

    pub fn height(&self) -> f64 {
        self.media_box.height()
    }

    pub fn effective_rect(&self) -> Rect {
        self.crop_box.unwrap_or(self.media_box)
    }
}

/// A content stream for a page or XObject form.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ContentStream {
    pub ops: Vec<Operation>,
}

impl ContentStream {
    pub fn new(ops: Vec<Operation>) -> Self {
        Self { ops }
    }
}

/// A node in the page tree: either a page or an intermediate node.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PageTreeNode {
    Node { kids: Vec<PageTreeNode> },
    Leaf(Box<Page>),
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

/// Document trailer metadata.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Trailer {
    pub root: u32,
    pub info: Option<u32>,
    pub id: Option<[String; 2]>,
    pub encrypt: Option<PdfValue>,
}

/// The top-level IR for a parsed PDF document.
///
/// This is an **immutable** snapshot: all fields are public but the type
/// provides no mutation methods. Editing is done through the
/// `tpt-glyph-pdf-editor` crate which operates via functional transformations.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Document {
    /// The version string from the PDF header (e.g. "1.4").
    pub version: String,
    /// The page tree, flattened for easy access.
    pub pages: Vec<Page>,
    /// Cross-reference table.
    pub xref: XRef,
    /// Trailer metadata.
    pub trailer: Trailer,
    /// All indirect objects, keyed by (object_number, generation).
    pub objects: Vec<(u32, u16, PdfValue)>,
}

impl Document {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn page(&self, index: usize) -> Option<&Page> {
        self.pages.get(index)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_basic_operations() {
        let r = Rect::new(0.0, 0.0, 100.0, 200.0);
        assert_eq!(r.width(), 100.0);
        assert_eq!(r.height(), 200.0);
        assert!(!r.is_empty());
        assert!(Rect::new(0.0, 0.0, 0.0, 0.0).is_empty());
    }

    #[test]
    fn matrix_identity() {
        assert_eq!(Matrix::IDENTITY.a, 1.0);
        assert_eq!(Matrix::IDENTITY.b, 0.0);
    }

    #[test]
    fn document_defaults() {
        let doc = Document {
            version: "1.7".into(),
            pages: Vec::new(),
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
        };
        assert_eq!(doc.page_count(), 0);
        assert!(doc.page(0).is_none());
    }

    #[test]
    fn page_effective_rect_falls_back_to_media_box() {
        let page = Page {
            index: 0,
            label: None,
            media_box: Rect::new(0.0, 0.0, 612.0, 792.0),
            crop_box: None,
            bleed_box: None,
            trim_box: None,
            art_box: None,
            rotate: 0,
            resources: Resources::empty(),
            contents: Vec::new(),
            annotations: Vec::new(),
            thumb: None,
            struct_parents: None,
        };
        assert_eq!(page.effective_rect(), page.media_box);
        assert_eq!(page.width(), 612.0);
        assert_eq!(page.height(), 792.0);
    }

    #[test]
    fn operation_display() {
        let op = Operation::MoveTo(10.0, 20.0);
        match op {
            Operation::MoveTo(x, y) => {
                assert!((x - 10.0).abs() < 1e-10);
                assert!((y - 20.0).abs() < 1e-10);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn content_stream_round_trip() {
        let ops = vec![
            Operation::Save,
            Operation::FillRgb(1.0, 0.0, 0.0),
            Operation::Rectangle(10.0, 10.0, 100.0, 100.0),
            Operation::Fill,
            Operation::Restore,
        ];
        let cs = ContentStream::new(ops.clone());
        assert_eq!(cs.ops.len(), 5);
        assert_eq!(cs.ops, ops);
    }

    #[test]
    fn resources_empty() {
        let r = Resources::empty();
        assert!(r.fonts.is_empty());
        assert!(r.xobjects.is_empty());
        assert!(r.ext_gstates.is_empty());
    }
}
