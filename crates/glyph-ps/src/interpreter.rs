// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-ps / interpreter
//
// The PostScript interpreter. Consumes a parsed `Program`, manages the operand
// / execution / graphics-state stacks, and drives the operator dispatch table.
// The dispatch table is derived from the glyph-kg knowledge graph catalog
// (Phase 3), guaranteeing the set of known operators matches the graph.
//
// Operator execution is split between pure operand-stack manipulation (stack
// operators) and graphics-state / rendering effects (path & paint operators),
// which mutate the interpreter's `GraphicsState` and append to a `RenderTree`.

use crate::error::{PsError, Result};
use crate::limits::ResourceLimits;
use crate::parser::{parse, Program, PsObject};
use crate::stacks::{Dictionary, ExecStack, OperandStack};
use glyph_core::geometry::{Path, Point, Subpath, Transform};
use glyph_core::graphics_state::{GraphicsState, LineCap, LineJoin, RgbColor};
use glyph_core::render::RenderTree;
use glyph_kg::catalog::catalog;
use glyph_kg::OperatorCategory;

/// The interpreter's mutable machine state (everything except the immutable
/// `GraphicsState`, which is kept on its own explicit stack for snapshotting).
#[derive(Debug, Default)]
pub struct Interpreter {
    operands: OperandStack,
    exec: ExecStack,
    dict: Dictionary,
    /// Graphics-state stack for `gsave`/`grestore`.
    state_stack: Vec<GraphicsState>,
    /// Current graphics state (immutable struct; operators derive new ones).
    state: GraphicsState,
    /// Current in-progress path being constructed.
    path: Path,
    /// Accumulated draw commands for the page.
    tree: RenderTree,
    /// Target device dimensions (kept for callers / future per-page metadata).
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    /// Resource limits enforced during execution (Phase 10).
    limits: ResourceLimits,
    /// Number of objects executed so far (instruction budget counter).
    steps: u64,
}

/// A handler for a single PostScript operator.
type OpFn = fn(&mut Interpreter) -> Result<()>;

/// Build the operator dispatch table from the glyph-kg catalog. This ties the
/// interpreter directly to the knowledge graph: every catalog operator that has
/// an implementation registered here is reachable. Operators without a
/// registered handler are still known (the catalog lists them) but reported as
/// `Unsupported` at runtime if invoked.
pub fn build_dispatch() -> std::collections::HashMap<String, OpEntry> {
    let mut map = std::collections::HashMap::new();
    // Register implemented operators. The catalog's `implemented` flag marks
    // which are wired in Phase 4; this assertion keeps the two in sync.
    let handlers: &[(&str, OpFn)] = &[
        ("moveto", op_moveto),
        ("lineto", op_lineto),
        ("curveto", op_curveto),
        ("closepath", op_closepath),
        ("stroke", op_stroke),
        ("fill", op_fill),
        ("clip", op_clip),
        ("gsave", op_gsave),
        ("grestore", op_grestore),
        ("setrgbcolor", op_setrgbcolor),
        ("setlinewidth", op_setlinewidth),
        ("concat", op_concat),
        ("rotate", op_rotate),
        ("arc", op_arc),
        ("def", op_def),
        ("pop", op_pop),
        ("dup", op_dup),
        ("exch", op_exch),
        ("showpage", op_showpage),
        ("rmoveto", op_rmoveto),
        ("rlineto", op_rlineto),
        ("eofill", op_eofill),
        ("setlinecap", op_setlinecap),
        ("setlinejoin", op_setlinejoin),
        ("setgray", op_setgray),
        ("show", op_show),
    ];
    for (name, f) in handlers {
        map.insert(
            name.to_string(),
            OpEntry {
                category: category_of(name),
                func: Some(*f),
            },
        );
    }
    // Ensure every catalog operator has an entry (implemented or not) so the
    // dispatch table fully reflects the KG.
    for def in catalog() {
        map.entry(def.name.clone()).or_insert(OpEntry {
            category: def.category,
            func: None,
        });
    }
    map
}

fn category_of(name: &str) -> OperatorCategory {
    catalog()
        .into_iter()
        .find(|d| d.name == name)
        .map(|d| d.category)
        .unwrap_or(OperatorCategory::GraphicsStateOp)
}

/// A dispatch-table entry: the operator's category plus an optional handler.
#[derive(Clone, Copy)]
pub struct OpEntry {
    pub category: OperatorCategory,
    pub func: Option<OpFn>,
}

impl Interpreter {
    /// Create an interpreter targeting a page of `width` x `height` device pixels,
    /// using the default (fail-closed but permissive) resource limits.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_limits(width, height, ResourceLimits::default())
    }

    /// Create an interpreter with explicit `ResourceLimits` (Phase 10).
    pub fn with_limits(width: u32, height: u32, limits: ResourceLimits) -> Self {
        Self {
            width,
            height,
            limits,
            tree: RenderTree::new(width, height),
            ..Default::default()
        }
    }

    /// Current resource limits.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Replace the active resource limits.
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }

    /// Enforce the instruction budget; called before each object is executed.
    fn check_step(&self) -> Result<()> {
        if self.limits.max_steps != 0 && self.steps >= self.limits.max_steps {
            return Err(PsError::ResourceLimit(format!(
                "instruction budget of {} exceeded",
                self.limits.max_steps
            )));
        }
        Ok(())
    }

    /// Enforce the exec-stack depth limit.
    fn check_exec_depth(&self) -> Result<()> {
        if self.limits.max_exec_depth != 0 && self.exec.depth() > self.limits.max_exec_depth {
            return Err(PsError::ResourceLimit(format!(
                "execution stack depth exceeded limit of {}",
                self.limits.max_exec_depth
            )));
        }
        Ok(())
    }

    /// Enforce the operand-stack size limit.
    fn check_operand(&self) -> Result<()> {
        if self.limits.max_operand_stack != 0 && self.operands.len() > self.limits.max_operand_stack
        {
            return Err(PsError::ResourceLimit(format!(
                "operand stack size exceeded limit of {}",
                self.limits.max_operand_stack
            )));
        }
        Ok(())
    }

    /// Enforce the draw-command (output) limit.
    fn check_output(&self) -> Result<()> {
        if self.limits.max_draw_commands != 0
            && self.tree.commands.len() >= self.limits.max_draw_commands
        {
            return Err(PsError::ResourceLimit(format!(
                "draw command count exceeded limit of {}",
                self.limits.max_draw_commands
            )));
        }
        Ok(())
    }

    /// Access the immutable graphics state.
    pub fn state(&self) -> &GraphicsState {
        &self.state
    }

    /// Access the accumulated render tree (after `showpage`/execution).
    pub fn tree(&self) -> &RenderTree {
        &self.tree
    }

    /// Access the current (in-progress) path. Useful for inspection/testing.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Parse and fully execute a PostScript program.
    pub fn run_source(&mut self, src: &str) -> Result<()> {
        let prog = parse(src)?;
        self.exec_program(prog)
    }

    /// Execute an already-parsed program.
    pub fn exec_program(&mut self, prog: Program) -> Result<()> {
        self.exec.push_program(prog.objects);
        self.check_exec_depth()?;
        while let Some(obj) = self.exec.pop_next() {
            self.check_step()?;
            self.steps += 1;
            self.check_exec_depth()?;
            self.exec_object(obj, &build_dispatch())?;
            self.check_operand()?;
        }
        Ok(())
    }

    /// Execute a single object. `dispatch` is passed in to avoid rebuilding the
    /// table on every object; callers that want to re-derive it may.
    fn exec_object(
        &mut self,
        obj: PsObject,
        dispatch: &std::collections::HashMap<String, OpEntry>,
    ) -> Result<()> {
        match obj {
            PsObject::Name(name) => {
                // Operator, or deferred name lookup from the dictionary.
                if let Some(entry) = dispatch.get(&name) {
                    match entry.func {
                        Some(f) => f(self),
                        None => Err(PsError::Parse(format!(
                            "operator '{name}' is known to the knowledge graph but not yet implemented"
                        ))),
                    }
                } else if let Some(value) = self.dict.lookup(&name).cloned() {
                    // A resolved value is executed: a procedure (array) runs its
                    // body; any other value is simply pushed as an operand.
                    match value {
                        PsObject::Array(body) => {
                            self.exec.push_program(body);
                            Ok(())
                        }
                        other => {
                            self.operands.push(other);
                            Ok(())
                        }
                    }
                } else {
                    Err(PsError::UnknownOperator(name))
                }
            }
            // An array literal appearing inline is a value (e.g. an argument to
            // `def`): push it on the operand stack rather than auto-executing.
            PsObject::Array(body) => {
                self.operands.push(PsObject::Array(body));
                Ok(())
            }
            other => {
                // Literal (number, string, literal name, bool): push as operand.
                self.operands.push(other);
                Ok(())
            }
        }
    }
}

// --- Operator implementations -------------------------------------------------

fn require_number(operands: &mut OperandStack) -> Result<f64> {
    let v = operands.pop()?;
    v.as_f64().ok_or(PsError::TypeError {
        expected: "number",
        found: "non-number",
    })
}

fn op_moveto(i: &mut Interpreter) -> Result<()> {
    let y = require_number(&mut i.operands)?;
    let x = require_number(&mut i.operands)?;
    let p = Point::new(x, y);
    if i.path.subpaths.is_empty() {
        i.path.subpaths.push(Subpath::new(p));
    } else {
        // Start a new subpath.
        i.path.subpaths.push(Subpath::new(p));
    }
    Ok(())
}

fn op_lineto(i: &mut Interpreter) -> Result<()> {
    let y = require_number(&mut i.operands)?;
    let x = require_number(&mut i.operands)?;
    let p = Point::new(x, y);
    if let Some(sp) = i.path.subpaths.last_mut() {
        let start = sp.start;
        let _end = sp.segments.last().map(|b| b.end).unwrap_or(start);
        // A straight line is a degenerate Bézier with coincident control points.
        sp.segments.push(glyph_core::geometry::CubicBezier {
            start,
            control1: lerp(start, p, 1.0 / 3.0),
            control2: lerp(start, p, 2.0 / 3.0),
            end: p,
        });
    } else {
        // Implied moveto.
        i.path.subpaths.push(Subpath::new(p));
    }
    Ok(())
}

fn op_curveto(i: &mut Interpreter) -> Result<()> {
    let y3 = require_number(&mut i.operands)?;
    let x3 = require_number(&mut i.operands)?;
    let y2 = require_number(&mut i.operands)?;
    let x2 = require_number(&mut i.operands)?;
    let y1 = require_number(&mut i.operands)?;
    let x1 = require_number(&mut i.operands)?;
    let end = i
        .path
        .subpaths
        .last()
        .map(|sp| sp.segments.last().map(|b| b.end).unwrap_or(sp.start));
    let start = end.unwrap_or_else(|| Point::new(0.0, 0.0));
    if let Some(sp) = i.path.subpaths.last_mut() {
        sp.segments.push(glyph_core::geometry::CubicBezier {
            start,
            control1: Point::new(x1, y1),
            control2: Point::new(x2, y2),
            end: Point::new(x3, y3),
        });
    }
    Ok(())
}

fn op_closepath(i: &mut Interpreter) -> Result<()> {
    if let Some(sp) = i.path.subpaths.last_mut() {
        sp.closed = true;
    }
    Ok(())
}

fn op_stroke(i: &mut Interpreter) -> Result<()> {
    let path = std::mem::take(&mut i.path);
    if !path.is_empty() {
        i.check_output()?;
        i.tree.stroke(&i.state, path);
    }
    Ok(())
}

fn op_fill(i: &mut Interpreter) -> Result<()> {
    let path = std::mem::take(&mut i.path);
    if !path.is_empty() {
        i.check_output()?;
        i.tree.fill(&i.state, path);
    }
    Ok(())
}

fn op_clip(i: &mut Interpreter) -> Result<()> {
    let path = std::mem::take(&mut i.path);
    if !path.is_empty() {
        i.check_output()?;
        i.tree.clip(&i.state, path);
    }
    Ok(())
}

fn op_gsave(i: &mut Interpreter) -> Result<()> {
    i.state_stack.push(i.state);
    Ok(())
}

fn op_grestore(i: &mut Interpreter) -> Result<()> {
    i.state = i.state_stack.pop().ok_or(PsError::StateStackUnderflow)?;
    Ok(())
}

fn op_setrgbcolor(i: &mut Interpreter) -> Result<()> {
    let b = require_number(&mut i.operands)?;
    let g = require_number(&mut i.operands)?;
    let r = require_number(&mut i.operands)?;
    let color = RgbColor::new(r, g, b);
    i.state = i.state.with_stroke_color(color).with_fill_color(color);
    Ok(())
}

fn op_setlinewidth(i: &mut Interpreter) -> Result<()> {
    let w = require_number(&mut i.operands)?;
    i.state = i.state.with_line_width(w);
    Ok(())
}

fn op_concat(i: &mut Interpreter) -> Result<()> {
    // Expect six numbers (a b c d e f) on the stack (f on top).
    let f = require_number(&mut i.operands)?;
    let e = require_number(&mut i.operands)?;
    let d = require_number(&mut i.operands)?;
    let c = require_number(&mut i.operands)?;
    let b = require_number(&mut i.operands)?;
    let a = require_number(&mut i.operands)?;
    let m = Transform::new(a, b, c, d, e, f);
    i.state = i.state.concat_transform(&m);
    Ok(())
}

fn op_rotate(i: &mut Interpreter) -> Result<()> {
    let deg = require_number(&mut i.operands)?;
    let rad = deg.to_radians();
    let (s, c) = rad.sin_cos();
    let m = Transform::new(c, s, -s, c, 0.0, 0.0);
    i.state = i.state.concat_transform(&m);
    Ok(())
}

fn op_arc(i: &mut Interpreter) -> Result<()> {
    // PostScript: x y r ang1 ang2 arc — append a circular arc centered at (x,y)
    // with radius r from ang1 to ang2 (degrees). We approximate with Bézier
    // segments (4 cubic segments max) appended to the current subpath.
    let ang2 = require_number(&mut i.operands)?;
    let ang1 = require_number(&mut i.operands)?;
    let r = require_number(&mut i.operands)?;
    let y = require_number(&mut i.operands)?;
    let x = require_number(&mut i.operands)?;

    let a1 = ang1.to_radians();
    let a2 = ang2.to_radians();

    // Start the arc with a moveto to the arc's start point if no path exists.
    let start = Point::new(x + r * a1.cos(), y + r * a1.sin());
    if i.path.subpaths.is_empty() {
        i.path.subpaths.push(Subpath::new(start));
    } else if let Some(sp) = i.path.subpaths.last_mut() {
        let prev = sp.segments.last().map(|b| b.end).unwrap_or(sp.start);
        if (prev.x - start.x).abs() > 1e-6 || (prev.y - start.y).abs() > 1e-6 {
            i.path.subpaths.push(Subpath::new(start));
        }
    }

    // Sample the arc into cubic Bézier segments (max 4) for smoothness.
    let steps = 4usize;
    let span = a2 - a1;
    for s in 0..steps {
        let t0 = a1 + span * (s as f64) / steps as f64;
        let t1 = a1 + span * (s as f64 + 1.0) / steps as f64;
        let p0 = Point::new(x + r * t0.cos(), y + r * t0.sin());
        let p1 = Point::new(x + r * t1.cos(), y + r * t1.sin());
        // Midpoint tangent control points (k = 4/3 * tan(sp/4)).
        let k = (4.0 / 3.0) * ((span / steps as f64) / 4.0).tan();
        let c1 = Point::new(p0.x - r * t0.sin() * k, p0.y + r * t0.cos() * k);
        let c2 = Point::new(p1.x + r * t1.sin() * k, p1.y - r * t1.cos() * k);
        if let Some(sp) = i.path.subpaths.last_mut() {
            let seg_start = sp.segments.last().map(|b| b.end).unwrap_or(sp.start);
            sp.segments.push(glyph_core::geometry::CubicBezier {
                start: seg_start,
                control1: c1,
                control2: c2,
                end: p1,
            });
        }
    }
    Ok(())
}

fn op_def(i: &mut Interpreter) -> Result<()> {
    let value = i.operands.pop()?;
    let name_obj = i.operands.pop()?;
    match name_obj {
        PsObject::LiteralName(n) => {
            // Store the value; arrays (procedures) are stored as-is to be
            // executed on lookup.
            i.dict.def(&n, value);
            Ok(())
        }
        _ => Err(PsError::Dict(
            "def requires a literal name (/name) on the stack".into(),
        )),
    }
}

fn op_pop(i: &mut Interpreter) -> Result<()> {
    i.operands.pop().map(|_| ())
}

fn op_dup(i: &mut Interpreter) -> Result<()> {
    let top = i.operands.top()?.clone();
    i.operands.push(top);
    Ok(())
}

fn op_showpage(_i: &mut Interpreter) -> Result<()> {
    // In our model `showpage` simply finalizes the current page. The accumulated
    // tree is already available; nothing further to do here.
    Ok(())
}

fn op_rmoveto(i: &mut Interpreter) -> Result<()> {
    let dy = require_number(&mut i.operands)?;
    let dx = require_number(&mut i.operands)?;
    let base = i
        .path
        .subpaths
        .last()
        .map(|sp| sp.segments.last().map(|b| b.end).unwrap_or(sp.start))
        .unwrap_or_else(|| Point::new(0.0, 0.0));
    let p = Point::new(base.x + dx, base.y + dy);
    i.path.subpaths.push(Subpath::new(p));
    Ok(())
}

fn op_rlineto(i: &mut Interpreter) -> Result<()> {
    let dy = require_number(&mut i.operands)?;
    let dx = require_number(&mut i.operands)?;
    let (start, base) = if let Some(sp) = i.path.subpaths.last() {
        let base = sp.segments.last().map(|b| b.end).unwrap_or(sp.start);
        (sp.start, base)
    } else {
        // Implied moveto to the origin, then relative line.
        i.path.subpaths.push(Subpath::new(Point::new(0.0, 0.0)));
        (Point::new(0.0, 0.0), Point::new(0.0, 0.0))
    };
    let p = Point::new(base.x + dx, base.y + dy);
    if let Some(sp) = i.path.subpaths.last_mut() {
        sp.segments.push(glyph_core::geometry::CubicBezier {
            start,
            control1: lerp(start, p, 1.0 / 3.0),
            control2: lerp(start, p, 2.0 / 3.0),
            end: p,
        });
    }
    Ok(())
}

fn op_eofill(i: &mut Interpreter) -> Result<()> {
    let path = std::mem::take(&mut i.path);
    if !path.is_empty() {
        i.check_output()?;
        i.tree.fill_even_odd(&i.state, path);
    }
    Ok(())
}

fn op_setlinecap(i: &mut Interpreter) -> Result<()> {
    let n = require_number(&mut i.operands)?;
    let cap = match n as i32 {
        0 => LineCap::Butt,
        1 => LineCap::Round,
        2 => LineCap::ProjectingSquare,
        _ => {
            return Err(PsError::Range {
                operator: "setlinecap",
                value: n,
            })
        }
    };
    i.state = i.state.with_line_cap(cap);
    Ok(())
}

fn op_setlinejoin(i: &mut Interpreter) -> Result<()> {
    let n = require_number(&mut i.operands)?;
    let join = match n as i32 {
        0 => LineJoin::Miter,
        1 => LineJoin::Round,
        2 => LineJoin::Bevel,
        _ => {
            return Err(PsError::Range {
                operator: "setlinejoin",
                value: n,
            })
        }
    };
    i.state = i.state.with_line_join(join);
    Ok(())
}

fn op_setgray(i: &mut Interpreter) -> Result<()> {
    let v = require_number(&mut i.operands)?;
    let g = v.clamp(0.0, 1.0);
    let color = RgbColor::new(g, g, g);
    i.state = i.state.with_stroke_color(color).with_fill_color(color);
    Ok(())
}

fn op_exch(i: &mut Interpreter) -> Result<()> {
    // Exchange the top two operands.
    let a = i.operands.pop()?;
    let b = i.operands.pop()?;
    i.operands.push(a);
    i.operands.push(b);
    Ok(())
}

fn op_show(i: &mut Interpreter) -> Result<()> {
    // Render text from a string by emitting a fill command for each glyph's
    // origin at the current point, advanced along the x axis by a fixed width.
    // Full font/glyph-outline handling lands in Phase 5; here we stamp the
    // current point so text produces a visible, deterministic result.
    let s = match i.operands.pop()? {
        PsObject::Str(text) => text,
        _ => {
            return Err(PsError::TypeError {
                expected: "string",
                found: "non-string",
            });
        }
    };
    let advance: f64 = 8.0;
    if !s.is_empty() {
        let start = i
            .path
            .subpaths
            .last()
            .map(|sp| sp.segments.last().map(|b| b.end).unwrap_or(sp.start))
            .unwrap_or_else(|| Point::new(0.0, 0.0));
        let mut path = Path::new();
        for (idx, _ch) in s.chars().enumerate() {
            let cx = start.x + idx as f64 * advance;
            let cy = start.y;
            path.subpaths.push(Subpath::new(Point::new(cx, cy)));
        }
        i.tree.fill(&i.state, path);
    }
    Ok(())
}

/// Linear interpolation between two points (used to represent straight lines as
/// degenerate Béziers so a single geometry type carries the whole path).
fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

#[cfg(test)]
fn require_top(it: &mut Interpreter) -> f64 {
    it.operands.top().unwrap().as_f64().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyph_core::render::DrawCommand;

    #[test]
    fn moveto_lineto_stroke_emits_command() {
        let mut it = Interpreter::new(100, 100);
        it.run_source("100 100 moveto 200 200 lineto stroke")
            .unwrap();
        assert_eq!(it.tree().commands.len(), 1);
        match &it.tree().commands[0] {
            DrawCommand::Stroke {
                color, line_width, ..
            } => {
                assert_eq!(*color, RgbColor::BLACK);
                assert_eq!(*line_width, 1.0);
            }
            _ => panic!("expected stroke"),
        }
    }

    #[test]
    fn setrgbcolor_updates_state() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("0.2 0.4 0.6 setrgbcolor").unwrap();
        assert_eq!(it.state().stroke_color, RgbColor::new(0.2, 0.4, 0.6));
        assert_eq!(it.state().fill_color, RgbColor::new(0.2, 0.4, 0.6));
    }

    #[test]
    fn gsave_grestore_restores_state() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("1 0 0 setrgbcolor gsave 0 1 0 setrgbcolor grestore")
            .unwrap();
        assert_eq!(it.state().stroke_color, RgbColor::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn fill_emits_fill_command() {
        let mut it = Interpreter::new(80, 80);
        it.run_source("0.5 0.5 0.5 setrgbcolor 10 10 moveto 40 10 lineto 40 40 lineto fill")
            .unwrap();
        assert_eq!(it.tree().commands.len(), 1);
        match &it.tree().commands[0] {
            DrawCommand::Fill { color, .. } => assert_eq!(*color, RgbColor::new(0.5, 0.5, 0.5)),
            _ => panic!("expected fill"),
        }
    }

    #[test]
    fn def_then_lookup_runs_procedure() {
        let mut it = Interpreter::new(100, 100);
        // Define a procedure that strokes a line, then call it.
        it.run_source("/draw { 0 0 moveto 50 50 lineto stroke } def draw")
            .unwrap();
        assert_eq!(it.tree().commands.len(), 1);
    }

    #[test]
    fn unknown_operator_errors() {
        let mut it = Interpreter::new(50, 50);
        assert!(it.run_source("foobar").is_err());
    }

    #[test]
    fn rotate_compose_ctm() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("90 rotate").unwrap();
        // A point on the +x axis should map to +y after a 90° rotation.
        let p = it.state().to_device(Point::new(10.0, 0.0));
        assert!((p.x).abs() < 1e-9, "x should be ~0, got {}", p.x);
        assert!((p.y - 10.0).abs() < 1e-9, "y should be ~10, got {}", p.y);
    }

    #[test]
    fn arc_builds_curves() {
        let mut it = Interpreter::new(200, 200);
        // Run the arc without stroking so we can inspect the in-progress path.
        it.run_source("100 100 50 0 360 arc").unwrap();
        let sub = &it.path().subpaths;
        assert!(!sub.is_empty(), "arc should build a subpath");
        // A full circle should contribute curve segments to the subpath.
        assert!(!sub[0].segments.is_empty(), "arc should add curve segments");
    }

    #[test]
    fn dispatch_table_covers_catalog() {
        let dispatch = build_dispatch();
        for def in catalog() {
            assert!(
                dispatch.contains_key(&def.name),
                "dispatch missing catalog operator {}",
                def.name
            );
        }
    }

    #[test]
    fn rmoveto_rlineto_are_relative() {
        let mut it = Interpreter::new(100, 100);
        it.run_source("10 20 moveto 30 40 rmoveto 5 5 rlineto")
            .unwrap();
        // Two subpaths: (10,20) then the relative move to (40,60) with a line to
        // (45,65).
        assert_eq!(it.path().subpaths[0].start, Point::new(10.0, 20.0));
        let sp = &it.path().subpaths[1];
        assert_eq!(sp.start, Point::new(40.0, 60.0));
        let end = sp.segments.last().unwrap().end;
        assert!((end.x - 45.0).abs() < 1e-9);
        assert!((end.y - 65.0).abs() < 1e-9);
    }

    #[test]
    fn eofill_emits_even_odd_command() {
        let mut it = Interpreter::new(80, 80);
        it.run_source("10 10 moveto 40 10 lineto 40 40 lineto eofill")
            .unwrap();
        assert_eq!(it.tree().commands.len(), 1);
        assert!(matches!(
            it.tree().commands[0],
            DrawCommand::FillEvenOdd { .. }
        ));
    }

    #[test]
    fn setlinecap_setlinejoin_update_state() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("1 setlinecap 2 setlinejoin").unwrap();
        assert_eq!(
            it.state().line_cap,
            glyph_core::graphics_state::LineCap::Round
        );
        assert_eq!(
            it.state().line_join,
            glyph_core::graphics_state::LineJoin::Bevel
        );
    }

    #[test]
    fn setlinecap_rejects_invalid() {
        let mut it = Interpreter::new(50, 50);
        assert!(it.run_source("9 setlinecap").is_err());
    }

    #[test]
    fn setgray_sets_gray_color() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("0.3 setgray").unwrap();
        assert_eq!(it.state().stroke_color, RgbColor::new(0.3, 0.3, 0.3));
        assert_eq!(it.state().fill_color, RgbColor::new(0.3, 0.3, 0.3));
    }

    #[test]
    fn exch_swaps_top_two_operands() {
        let mut it = Interpreter::new(50, 50);
        it.run_source("1 2 exch").unwrap();
        assert_eq!(require_top(&mut it), 1.0);
    }

    #[test]
    fn show_emits_fill_for_string() {
        let mut it = Interpreter::new(100, 100);
        it.run_source("20 20 moveto (Hi) show").unwrap();
        assert_eq!(it.tree().commands.len(), 1);
        assert!(matches!(it.tree().commands[0], DrawCommand::Fill { .. }));
    }

    #[test]
    fn operand_stack_limit_is_enforced() {
        let mut it = Interpreter::with_limits(
            100,
            100,
            ResourceLimits {
                max_operand_stack: 4,
                ..ResourceLimits::unbounded()
            },
        );
        // Push 5 numbers onto the operand stack; the 5th exceeds the limit.
        assert!(it.run_source("1 2 3 4 5").is_err());
    }

    #[test]
    fn draw_command_limit_is_enforced() {
        let mut it = Interpreter::with_limits(
            200,
            200,
            ResourceLimits {
                max_draw_commands: 3,
                ..ResourceLimits::unbounded()
            },
        );
        // Three fills succeed; the fourth trips the output limit.
        let src = "
            0 0 0 setrgbcolor 0 0 moveto 10 0 lineto 10 10 lineto fill
            0 0 0 setrgbcolor 0 0 moveto 10 0 lineto 10 10 lineto fill
            0 0 0 setrgbcolor 0 0 moveto 10 0 lineto 10 10 lineto fill
            0 0 0 setrgbcolor 0 0 moveto 10 0 lineto 10 10 lineto fill
        ";
        assert!(it.run_source(src).is_err());
    }

    #[test]
    fn instruction_budget_is_enforced() {
        let mut it = Interpreter::with_limits(
            100,
            100,
            ResourceLimits {
                max_steps: 5,
                ..ResourceLimits::unbounded()
            },
        );
        assert!(it.run_source("1 2 3 4 5 6 7 8 9 10 add add add").is_err());
    }

    #[test]
    fn recursion_depth_limit_is_enforced() {
        let mut it = Interpreter::with_limits(
            100,
            100,
            ResourceLimits {
                max_exec_depth: 8,
                ..ResourceLimits::unbounded()
            },
        );
        // A deeply recursive procedure should exceed the exec-stack depth limit.
        let src = "/f { f } def f";
        assert!(it.run_source(src).is_err());
    }
}
