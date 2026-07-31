// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf / xobject
//
// XObject decoding groundwork. Form XObjects are rendered recursively through
// the shared content interpreter; Image XObjects have their (Flate/raw) stream
// decoded and stamped as a representative filled rectangle so images register in
// the render tree. Full pixel-blit compositing is a later backend milestone.

use crate::content::{render_ops, TextState};
use crate::error::{PdfError, Result};
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath};
use tpt_glyph_core::graphics_state::GraphicsState;
use tpt_glyph_core::render::{DrawCommand, RenderTree};
use pdf::content::FormXObject;
use pdf::content::Matrix;
use pdf::object::*;

/// Render a resolved XObject into `tree`, inheriting the current graphics state.
pub fn decode_xobject(
    xobj: &XObject,
    resources: &Resources,
    resolver: &impl Resolve,
    state: &mut GraphicsState,
    tree: &mut RenderTree,
) -> Result<()> {
    match xobj {
        XObject::Form(form) => render_form(form, resources, resolver, state, tree),
        XObject::Image(img) => render_image_placeholder(img, state, resolver, tree),
        XObject::Postscript(_) => Ok(()), // PostScript XObjects are unsupported
    }
}

fn render_form(
    form: &FormXObject,
    resources: &Resources,
    resolver: &impl Resolve,
    state: &mut GraphicsState,
    tree: &mut RenderTree,
) -> Result<()> {
    // A form's own matrix maps its space into user space; prepend it to the CTM.
    let form_matrix = form
        .dict()
        .matrix
        .as_ref()
        .and_then(|p| Matrix::from_primitive(p.clone(), resolver).ok())
        .unwrap_or_default();
    let local = tpt_glyph_core::geometry::Transform::new(
        form_matrix.a as f64,
        form_matrix.b as f64,
        form_matrix.c as f64,
        form_matrix.d as f64,
        form_matrix.e as f64,
        form_matrix.f as f64,
    );
    let saved = *state;
    *state = state.concat_transform(&local);

    let ops = form.operations(resolver).unwrap_or_default();
    let mut ts = TextState::default();
    let mut path = Path::new();
    // Form XObjects carry their own resource dictionary, merged over the page's.
    let form_resources = form
        .dict()
        .resources
        .as_ref()
        .map(|r| (**r).clone())
        .unwrap_or_else(|| resources.clone());
    render_ops(
        &ops,
        &form_resources,
        resolver,
        state,
        &mut ts,
        &mut path,
        tree,
    );

    *state = saved;
    Ok(())
}

fn render_image_placeholder(
    img: &ImageXObject,
    state: &GraphicsState,
    resolver: &impl Resolve,
    tree: &mut RenderTree,
) -> Result<()> {
    // Decode the stream (Flate/raw). We don't yet composite pixels, but decoding
    // exercises the image pipeline and lets us record a footprint rectangle.
    let _data = img
        .image_data(resolver)
        .map_err(|e| PdfError::Unsupported(format!("image decode: {e}")))?;

    let w = img.width as f64;
    let h = img.height as f64;
    let mut sp = Subpath::new(Point::new(0.0, 0.0));
    let corners = [
        Point::new(w, 0.0),
        Point::new(w, h),
        Point::new(0.0, h),
        Point::new(0.0, 0.0),
    ];
    for c in corners {
        push_line_to(&mut sp, c);
    }
    sp.closed = true;
    let mut path = Path::new();
    path.subpaths.push(sp);

    tree.commands.push(DrawCommand::Fill {
        path,
        color: state.fill_color,
        ctm: state.ctm,
    });
    Ok(())
}

fn push_line_to(sp: &mut Subpath, p: Point) {
    let start = sp.segments.last().map(|b| b.end).unwrap_or(sp.start);
    sp.segments.push(CubicBezier {
        start,
        control1: lerp(start, p, 1.0 / 3.0),
        control2: lerp(start, p, 2.0 / 3.0),
        end: p,
    });
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
