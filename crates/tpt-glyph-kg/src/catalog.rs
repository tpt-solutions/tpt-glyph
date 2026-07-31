// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-kg / catalog
//
// The declarative catalog of PostScript operators. Each entry names the operator,
// its functional category, the graphics-state attributes it touches (the isolated
// sub-graph), and the pixel-buffer effects it produces. This catalog is the
// source from which the knowledge graph is ingested.

use crate::{OperatorCategory, OperatorDef};

/// The canonical PostScript operator catalog. `implemented` reflects the current
/// state of the interpreter (Phase 4+); operators not yet implemented are listed
/// so coverage can be tracked.
pub fn catalog() -> Vec<OperatorDef> {
    use OperatorCategory::*;
    vec![
        // --- Path construction ---
        op(
            "moveto",
            PathConstruction,
            "Begin a new subpath at (x,y).",
            &[],
            &[],
            true,
        ),
        op(
            "lineto",
            PathConstruction,
            "Append a straight line to (x,y).",
            &[],
            &[],
            true,
        ),
        op(
            "curveto",
            PathConstruction,
            "Append a cubic Bézier via two control points.",
            &[],
            &[],
            true,
        ),
        op(
            "closepath",
            PathConstruction,
            "Close the current subpath.",
            &[],
            &[],
            true,
        ),
        op(
            "rmoveto",
            PathConstruction,
            "Relative moveto.",
            &[],
            &[],
            true,
        ),
        op(
            "rlineto",
            PathConstruction,
            "Relative lineto.",
            &[],
            &[],
            true,
        ),
        op(
            "arc",
            PathConstruction,
            "Append a circular arc.",
            &["ctm"],
            &[],
            true,
        ),
        // --- Path painting ---
        op(
            "stroke",
            PathPainting,
            "Stroke the current path.",
            &["stroke_color", "line_width", "ctm", "clip_path"],
            &["paints_path"],
            true,
        ),
        op(
            "fill",
            PathPainting,
            "Fill the current path (nonzero winding).",
            &["fill_color", "ctm", "clip_path"],
            &["paints_path"],
            true,
        ),
        op(
            "clip",
            PathPainting,
            "Intersect clipping path with current path.",
            &["clip_path"],
            &[],
            true,
        ),
        op(
            "eofill",
            PathPainting,
            "Fill using even-odd rule.",
            &["fill_color", "ctm", "clip_path"],
            &["paints_path"],
            true,
        ),
        // --- Graphics state ---
        op(
            "gsave",
            GraphicsStateOp,
            "Push a copy of the graphics state.",
            &[
                "stroke_color",
                "fill_color",
                "line_width",
                "ctm",
                "clip_path",
            ],
            &[],
            true,
        ),
        op(
            "grestore",
            GraphicsStateOp,
            "Pop and restore the graphics state.",
            &[
                "stroke_color",
                "fill_color",
                "line_width",
                "ctm",
                "clip_path",
            ],
            &[],
            true,
        ),
        op(
            "setrgbcolor",
            GraphicsStateOp,
            "Set stroke and fill color from RGB.",
            &["stroke_color", "fill_color"],
            &[],
            true,
        ),
        op(
            "setlinewidth",
            GraphicsStateOp,
            "Set the line width.",
            &["line_width"],
            &[],
            true,
        ),
        op(
            "concat",
            GraphicsStateOp,
            "Concatenate a matrix onto the CTM.",
            &["ctm"],
            &[],
            true,
        ),
        op(
            "rotate",
            GraphicsStateOp,
            "Rotate the CTM by degrees about the origin.",
            &["ctm"],
            &[],
            true,
        ),
        op(
            "setlinecap",
            GraphicsStateOp,
            "Set line cap style.",
            &["line_cap"],
            &[],
            true,
        ),
        op(
            "setlinejoin",
            GraphicsStateOp,
            "Set line join style.",
            &["line_join"],
            &[],
            true,
        ),
        op(
            "setgray",
            GraphicsStateOp,
            "Set color to a gray level.",
            &["stroke_color", "fill_color"],
            &[],
            true,
        ),
        // --- Stack / dictionary ---
        op(
            "def",
            Stack,
            "Define a name in the dictionary stack.",
            &[],
            &[],
            true,
        ),
        op(
            "pop",
            Stack,
            "Remove the top operand-stack element.",
            &[],
            &[],
            true,
        ),
        op(
            "dup",
            Stack,
            "Duplicate the top operand-stack element.",
            &[],
            &[],
            true,
        ),
        op(
            "exch",
            Stack,
            "Exchange the top two stack elements.",
            &[],
            &[],
            true,
        ),
        // --- Device / show ---
        op(
            "showpage",
            Device,
            "Render the current page to the device.",
            &["clip_path"],
            &["flushes_page"],
            true,
        ),
        op(
            "show",
            Device,
            "Render text from a string.",
            &["fill_color", "ctm"],
            &["paints_path"],
            true,
        ),
    ]
}

fn op(
    name: &str,
    category: OperatorCategory,
    description: &str,
    affects_state: &[&str],
    produces_effects: &[&str],
    implemented: bool,
) -> OperatorDef {
    OperatorDef {
        name: name.to_string(),
        category,
        description: description.to_string(),
        affects_state: affects_state.iter().map(|s| s.to_string()).collect(),
        produces_effects: produces_effects.iter().map(|s| s.to_string()).collect(),
        implemented,
    }
}
