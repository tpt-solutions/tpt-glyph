// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf / content
//
// Maps PDF content-stream operators (`pdf::content::Op`) onto the shared
// tpt-glyph-core rendering pipeline. The interpreter maintains an immutable
// `GraphicsState` (reused from tpt-glyph-core), a per-page `TextState`, the
// in-progress `Path`, and the accumulated `RenderTree`.

use crate::error::{PdfError, Result};
use crate::fonts::decode_pdf_string;
use crate::xobject::decode_xobject;
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath, Transform};
use tpt_glyph_core::graphics_state::{GraphicsState, LineCap, LineJoin, RgbColor};
use tpt_glyph_core::render::{DrawCommand, RenderTree};
use pdf::content::{
    Color, LineCap as PdfLineCap, LineJoin as PdfLineJoin, Matrix, Op, TextDrawAdjusted, Winding,
};
use pdf::font::Widths as PdfWidths;
use pdf::object::*;
use pdf::primitive::Name;
use pdf::primitive::PdfString;

/// Text-state parameters for a text object (`BT` … `ET`).
///
/// Position is tracked in *text space*; the effective text position in user
/// space is derived from the text matrix (`tm`) and the start-of-line matrix
/// (`tlm`), per the PDF specification.
#[derive(Debug, Clone)]
pub struct TextState {
    /// Font resource name currently selected (`Tf`).
    pub font: Option<String>,
    /// Font size in points (`Tf`).
    pub font_size: f32,
    /// Character spacing (`Tc`).
    pub char_spacing: f32,
    /// Word spacing (`Tw`).
    pub word_spacing: f32,
    /// Horizontal text scaling (`Tz`), fraction of 100%.
    pub scale: f32,
    /// Leading (`TL`).
    pub leading: f32,
    /// Text rise (`Ts`).
    pub rise: f32,
    /// Render mode (`Tr`).
    pub render_mode: u8,
    /// Current text matrix (maps text space → user space).
    pub tm: Matrix,
    /// Text line matrix (start of current line).
    pub tlm: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 100.0,
            leading: 0.0,
            rise: 0.0,
            render_mode: 0,
            tm: Matrix::default(),
            tlm: Matrix::default(),
        }
    }
}

fn pdf_point_to_ours(p: &pdf::content::Point) -> Point {
    Point::new(p.x as f64, p.y as f64)
}

fn pdf_color_to_rgb(c: &Color) -> RgbColor {
    match *c {
        Color::Gray(g) => RgbColor::new(g as f64, g as f64, g as f64),
        Color::Rgb(rgb) => RgbColor::new(rgb.red as f64, rgb.green as f64, rgb.blue as f64),
        // CMYK → simple naive conversion (ignores undercolor removal).
        Color::Cmyk(cmyk) => {
            let r = (1.0 - cmyk.cyan) * (1.0 - cmyk.key);
            let g = (1.0 - cmyk.magenta) * (1.0 - cmyk.key);
            let b = (1.0 - cmyk.yellow) * (1.0 - cmyk.key);
            RgbColor::new(r as f64, g as f64, b as f64)
        }
        Color::Other(_) => RgbColor::BLACK,
    }
}

fn pdf_line_cap(c: PdfLineCap) -> LineCap {
    match c {
        PdfLineCap::Butt => LineCap::Butt,
        PdfLineCap::Round => LineCap::Round,
        PdfLineCap::Square => LineCap::ProjectingSquare,
    }
}

fn pdf_line_join(j: PdfLineJoin) -> LineJoin {
    match j {
        PdfLineJoin::Miter => LineJoin::Miter,
        PdfLineJoin::Round => LineJoin::Round,
        PdfLineJoin::Bevel => LineJoin::Bevel,
    }
}

/// Convert the `pdf::object` line-cap enum (used by ExtGState) to our own.
fn object_line_cap(c: pdf::object::LineCap) -> LineCap {
    match c {
        pdf::object::LineCap::Butt => LineCap::Butt,
        pdf::object::LineCap::Round => LineCap::Round,
        pdf::object::LineCap::Square => LineCap::ProjectingSquare,
    }
}

/// Convert the `pdf::object` line-join enum (used by ExtGState) to our own.
fn object_line_join(j: pdf::object::LineJoin) -> LineJoin {
    match j {
        pdf::object::LineJoin::Miter => LineJoin::Miter,
        pdf::object::LineJoin::Round => LineJoin::Round,
        pdf::object::LineJoin::Bevel => LineJoin::Bevel,
    }
}

fn winding_to_even_odd(w: Winding) -> bool {
    matches!(w, Winding::EvenOdd)
}

/// Render a full page's content stream. `resolver` is needed to load fonts and
/// XObjects referenced indirectly by the page's `Resources`.
pub fn render_ops(
    ops: &[Op],
    resources: &Resources,
    resolver: &impl Resolve,
    state: &mut GraphicsState,
    ts: &mut TextState,
    path: &mut Path,
    tree: &mut RenderTree,
) {
    // q/Q graphics-state stack: each `Save` pushes a clone, `Restore` pops.
    let mut gstack: Vec<GraphicsState> = Vec::new();
    for op in ops {
        match op {
            Op::Save => gstack.push(*state),
            Op::Restore => {
                if let Some(s) = gstack.pop() {
                    *state = s;
                }
            }
            _ => {
                let _ = exec_op(op, resources, resolver, state, ts, path, tree);
            }
        }
    }
}

/// Execute a single content-stream operator, mutating the running state.
#[allow(clippy::too_many_arguments)]
fn exec_op(
    op: &Op,
    resources: &Resources,
    resolver: &impl Resolve,
    state: &mut GraphicsState,
    ts: &mut TextState,
    path: &mut Path,
    tree: &mut RenderTree,
) -> Result<()> {
    match *op {
        // --- Path construction ---
        Op::MoveTo { p } => {
            path.subpaths.push(Subpath::new(pdf_point_to_ours(&p)));
        }
        Op::LineTo { p } => {
            push_line(path, pdf_point_to_ours(&p));
        }
        Op::CurveTo { c1, c2, p } => {
            let start = current_point(path);
            if let Some(sp) = path.subpaths.last_mut() {
                sp.push_curve(CubicBezier {
                    start,
                    control1: pdf_point_to_ours(&c1),
                    control2: pdf_point_to_ours(&c2),
                    end: pdf_point_to_ours(&p),
                });
            }
        }
        Op::Rect { rect } => {
            let lb = Point::new(rect.x as f64, rect.y as f64);
            let rt = Point::new((rect.x + rect.width) as f64, (rect.y + rect.height) as f64);
            let rb = Point::new((rect.x + rect.width) as f64, rect.y as f64);
            let lt = Point::new(rect.x as f64, (rect.y + rect.height) as f64);
            let mut sp = Subpath::new(lb);
            push_line_to(&mut sp, rb);
            push_line_to(&mut sp, rt);
            push_line_to(&mut sp, lt);
            sp.closed = true;
            path.subpaths.push(sp);
        }
        Op::Close => {
            if let Some(sp) = path.subpaths.last_mut() {
                sp.closed = true;
            }
        }
        Op::EndPath => {
            *path = Path::new();
        }

        // --- Painting ---
        Op::Stroke => emit_stroke(state, path, tree),
        Op::Fill { winding } => emit_fill(state, path, tree, winding_to_even_odd(winding)),
        Op::FillAndStroke { winding } => {
            emit_fill(state, path, tree, winding_to_even_odd(winding));
            emit_stroke(state, path, tree);
        }
        Op::Clip { winding } => {
            let eo = winding_to_even_odd(winding);
            emit_clip(state, path, tree, eo);
        }
        Op::Shade { .. } => { /* shading patterns: not yet rendered */ }

        // --- Graphics state (stack) handled by caller ---

        // --- CTM ---
        Op::Transform { ref matrix } => {
            *state = state.concat_transform(&to_transform(matrix));
        }

        // --- Line attributes ---
        Op::LineWidth { width } => *state = state.with_line_width(width as f64),
        Op::Dash { .. } => { /* dashing not yet rendered */ }
        Op::LineJoin { join } => *state = state.with_line_join(pdf_line_join(join)),
        Op::LineCap { cap } => *state = state.with_line_cap(pdf_line_cap(cap)),
        Op::MiterLimit { .. } => { /* cosmetic */ }
        Op::Flatness { .. } => { /* cosmetic */ }
        Op::GraphicsState { ref name } => apply_ext_gstate(resources, name, state)?,

        // --- Color ---
        Op::StrokeColor { ref color } => {
            *state = state.with_stroke_color(pdf_color_to_rgb(color));
        }
        Op::FillColor { ref color } => {
            *state = state.with_fill_color(pdf_color_to_rgb(color));
        }
        Op::FillColorSpace { .. } => { /* color space selection */ }
        Op::StrokeColorSpace { .. } => {}
        Op::RenderingIntent { .. } => {}

        // --- Text ---
        Op::BeginText => {
            ts.tm = Matrix::default();
            ts.tlm = Matrix::default();
        }
        Op::EndText => {}
        Op::CharSpacing { char_space } => ts.char_spacing = char_space,
        Op::WordSpacing { word_space } => ts.word_spacing = word_space,
        Op::TextScaling { horiz_scale } => ts.scale = horiz_scale,
        Op::Leading { leading } => ts.leading = leading,
        Op::TextFont { ref name, size } => {
            ts.font = Some(format!("{name}"));
            ts.font_size = size;
        }
        Op::TextRenderMode { mode } => ts.render_mode = mode as u8,
        Op::TextRise { rise } => ts.rise = rise,
        Op::MoveTextPosition { translation } => {
            let tx = translation.x as f64;
            let ty = translation.y as f64;
            ts.tm = translate_matrix(&ts.tlm, tx, ty);
        }
        Op::SetTextMatrix { matrix } => {
            ts.tm = matrix;
            ts.tlm = matrix;
        }
        Op::TextNewline => {
            ts.tm = translate_matrix(&ts.tlm, 0.0, -(ts.leading as f64));
            ts.tlm = ts.tm;
        }
        Op::TextDraw { ref text } => draw_text(text, resources, resolver, state, ts, tree)?,
        Op::TextDrawAdjusted { ref array } => {
            for seg in array {
                match *seg {
                    TextDrawAdjusted::Text(ref t) => {
                        draw_text(t, resources, resolver, state, ts, tree)?
                    }
                    TextDrawAdjusted::Spacing(dx) => {
                        // `dx` is in thousandths of a unit of text space.
                        let factor = (ts.font_size as f64) * (ts.scale as f64 / 100.0);
                        let step = factor * dx as f64 / 1000.0;
                        ts.tm = translate_matrix(&ts.tm, step, 0.0);
                    }
                }
            }
        }

        // --- Marked content (ignored for rendering) ---
        Op::BeginMarkedContent { .. } | Op::EndMarkedContent | Op::MarkedContentPoint { .. } => {}

        // --- XObjects ---
        Op::XObject { ref name } => {
            if let Some(ref_xobj) = resources.xobjects.get(name) {
                let xobj = resolver.get(*ref_xobj)?;
                decode_xobject(&xobj, resources, resolver, state, tree)?;
            }
        }

        Op::InlineImage { .. } => { /* inline image: handled as groundwork */ }

        // Save/Restore are handled by the caller's graphics-state stack and are
        // never dispatched here; keep the match exhaustive.
        Op::Save | Op::Restore => {}
    }
    Ok(())
}

// --- helpers -----------------------------------------------------------------

fn to_transform(m: &Matrix) -> Transform {
    Transform::new(
        m.a as f64, m.b as f64, m.c as f64, m.d as f64, m.e as f64, m.f as f64,
    )
}

fn current_point(path: &Path) -> Point {
    path.subpaths
        .last()
        .map(|sp| sp.segments.last().map(|b| b.end).unwrap_or(sp.start))
        .unwrap_or_else(|| Point::new(0.0, 0.0))
}

fn push_line(path: &mut Path, p: Point) {
    if let Some(sp) = path.subpaths.last_mut() {
        push_line_to(sp, p);
    } else {
        path.subpaths.push(Subpath::new(p));
    }
}

fn push_line_to(sp: &mut Subpath, p: Point) {
    let start = sp.segments.last().map(|b| b.end).unwrap_or(sp.start);
    sp.segments.push(degenerate_line(start, p));
}

fn degenerate_line(start: Point, end: Point) -> CubicBezier {
    CubicBezier {
        start,
        control1: lerp(start, end, 1.0 / 3.0),
        control2: lerp(start, end, 2.0 / 3.0),
        end,
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn emit_stroke(state: &GraphicsState, path: &mut Path, tree: &mut RenderTree) {
    let p = std::mem::take(path);
    if !p.is_empty() {
        tree.commands.push(DrawCommand::Stroke {
            path: p,
            color: state.stroke_color,
            line_width: state.line_width,
            ctm: state.ctm,
        });
    }
}

fn emit_fill(state: &GraphicsState, path: &mut Path, tree: &mut RenderTree, even_odd: bool) {
    let p = std::mem::take(path);
    if !p.is_empty() {
        let cmd = if even_odd {
            DrawCommand::FillEvenOdd {
                path: p,
                color: state.fill_color,
                ctm: state.ctm,
            }
        } else {
            DrawCommand::Fill {
                path: p,
                color: state.fill_color,
                ctm: state.ctm,
            }
        };
        tree.commands.push(cmd);
    }
}

fn emit_clip(state: &GraphicsState, path: &mut Path, tree: &mut RenderTree, _even_odd: bool) {
    let p = std::mem::take(path);
    if !p.is_empty() {
        tree.commands.push(DrawCommand::Clip {
            path: p,
            ctm: state.ctm,
        });
    }
}

fn apply_ext_gstate(resources: &Resources, name: &Name, state: &mut GraphicsState) -> Result<()> {
    let gs = resources
        .graphics_states
        .get(name)
        .ok_or_else(|| PdfError::Unsupported(format!("missing ExtGState /{name}")))?;
    if let Some(w) = gs.line_width {
        *state = state.with_line_width(w as f64);
    }
    if let Some(cap) = gs.line_cap {
        *state = state.with_line_cap(object_line_cap(cap));
    }
    if let Some(join) = gs.line_join {
        *state = state.with_line_join(object_line_join(join));
    }
    Ok(())
}

fn draw_text(
    text: &PdfString,
    resources: &Resources,
    resolver: &impl Resolve,
    state: &mut GraphicsState,
    ts: &mut TextState,
    tree: &mut RenderTree,
) -> Result<()> {
    let font_name = match ts.font.clone() {
        Some(f) => f,
        None => return Ok(()),
    };
    let name = Name::from(font_name.as_str());
    // Resolve the font and extract its glyph-width table (in em units).
    let widths: Option<pdf::font::Widths> = resources
        .fonts
        .get(&name)
        .and_then(|lazy| lazy.load(resolver).ok())
        .and_then(|mr| mr.widths(resolver).ok().flatten());

    let s = decode_pdf_string(text);
    let scale = ts.scale as f64 / 100.0;
    let advance_unit = ts.font_size as f64 * scale;

    let mut cursor = ts.tm;
    for ch in s.chars() {
        let origin = text_space_cursor(&cursor);
        if ts.render_mode != 3 {
            // Glyph placeholder: a small filled box at the glyph origin. Full
            // glyph-outline compositing is a later milestone; this keeps text
            // visible and deterministic for the diff harness.
            let size = (ts.font_size as f64 * 0.6).max(1.0);
            let base_y = origin.y + (ts.font_size as f64 * 0.1);
            let mut box_sp = Subpath::new(Point::new(origin.x, base_y));
            push_line_to(&mut box_sp, Point::new(origin.x + size, base_y));
            push_line_to(&mut box_sp, Point::new(origin.x + size, base_y + size));
            push_line_to(&mut box_sp, Point::new(origin.x, base_y + size));
            box_sp.closed = true;
            let mut gpath = Path::new();
            gpath.subpaths.push(box_sp);
            tree.commands.push(DrawCommand::Fill {
                path: gpath,
                color: state.fill_color,
                ctm: state.ctm,
            });
        }
        let w = glyph_advance(widths.as_ref(), ch) + ts.char_spacing as f64;
        let dx = w * advance_unit;
        cursor = translate_matrix(&cursor, dx, 0.0);
    }
    ts.tm = cursor;
    Ok(())
}

fn glyph_advance(widths: Option<&PdfWidths>, ch: char) -> f64 {
    match widths {
        Some(w) => w.get(ch as usize).max(0.0) as f64,
        None => 0.5,
    }
}

/// A `pdf::content::Matrix` helper: translate by (tx,ty).
fn translate_matrix(m: &Matrix, tx: f64, ty: f64) -> Matrix {
    Matrix {
        a: m.a,
        b: m.b,
        c: m.c,
        d: m.d,
        e: m.e + tx as f32,
        f: m.f + ty as f32,
    }
}

/// Text-space cursor origin (in user space) from a text matrix.
fn text_space_cursor(m: &Matrix) -> Point {
    Point::new(m.e as f64, m.f as f64)
}
