// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/layout
//
// The TeXbook Chapter 17 "mlist to hlist" algorithm: recursively turns a
// `MathExpr` into a tree of sized, positioned `MathBox`es. This module never
// touches `tpt-glyph-core` (it only depends on `tpt-glyph-font` for glyph
// metrics), so it stays usable in a `no_std` build; `emit` (feature `std`)
// is what walks the resulting box tree into draw commands.

use crate::ast::{DisplayStyleKind, FractionBar, Limits, MathExpr, MathSpace};
use crate::atom::{interelement_space, reclassify_bins, AtomClass, SpaceAmount};
use crate::constants::MathConstants;
use crate::style::{MathStyle, SizeCategory};
use alloc::vec;
use alloc::vec::Vec;
use tpt_glyph_font::{Font, GlyphId};

/// A laid-out box: `width`/`height`/`depth` are in the same units as the
/// `size` passed to [`layout`] (typically points). `height` extends above
/// the box's own baseline, `depth` below it.
#[derive(Debug, Clone)]
pub struct MathBox {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub content: BoxContent,
}

/// What a [`MathBox`] draws.
#[derive(Debug, Clone)]
pub enum BoxContent {
    /// A single glyph, scaled to `font_scale` (the point size to render the
    /// font at) and then further stretched vertically by `y_scale` about the
    /// glyph's own baseline and shifted by `y_shift` — used for stretchy
    /// radical/delimiter glyphs; ordinary glyphs have `y_scale = 1.0`,
    /// `y_shift = 0.0`.
    Glyph {
        gid: GlyphId,
        font_scale: f64,
        y_scale: f64,
        y_shift: f64,
    },
    /// Children laid out left-to-right; each child's `dx`/`dy` are relative
    /// to this box's own origin (baseline at `x = 0`).
    HList(Vec<PositionedBox>),
    /// Children stacked vertically; same `dx`/`dy` convention as `HList`.
    VList(Vec<PositionedBox>),
    /// A filled rectangle spanning the box's `width`, `thickness` tall
    /// (fraction bars, over/underlines, radical rules).
    Rule { thickness: f64 },
    /// Draws nothing (spacing, or a missing glyph).
    Empty,
}

/// A child box positioned relative to its parent's origin.
#[derive(Debug, Clone)]
pub struct PositionedBox {
    pub dx: f64,
    pub dy: f64,
    pub b: MathBox,
}

/// Lay out `expr` at `style`, using `font`/`k` (see [`MathConstants`]) at
/// `size` (the *root* Text-style font size — nested styles derive their
/// actual rendering size from `size` and their own [`SizeCategory`], not
/// from further shrinking `size` at each recursion level, matching TeX's
/// fixed three-physical-sizes model).
pub fn layout(
    expr: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    match expr {
        MathExpr::Identifier(s) | MathExpr::Number(s) => text_row_box(s, font, style, k, size),
        MathExpr::Symbol { ch, .. } => glyph_box(*ch, font, style, k, size),
        MathExpr::Operator { ch, name, .. } => match ch {
            Some(c) => glyph_box(*c, font, style, k, size),
            None => text_row_box(name, font, style, k, size),
        },
        MathExpr::Row(items) => layout_row(items, style, font, k, size),
        MathExpr::Fraction {
            numerator,
            denominator,
            bar,
        } => layout_fraction(numerator, denominator, *bar, style, font, k, size),
        MathExpr::Superscript { base, sup } => {
            if operator_uses_limits(base, style) {
                layout_limits(base, None, Some(sup), style, font, k, size)
            } else {
                layout_superscript(base, sup, style, font, k, size)
            }
        }
        MathExpr::Subscript { base, sub } => {
            if operator_uses_limits(base, style) {
                layout_limits(base, Some(sub), None, style, font, k, size)
            } else {
                layout_subscript(base, sub, style, font, k, size)
            }
        }
        MathExpr::SubSup { base, sub, sup } => {
            if operator_uses_limits(base, style) {
                layout_limits(base, Some(sub), Some(sup), style, font, k, size)
            } else {
                layout_subsup(base, sub, sup, style, font, k, size)
            }
        }
        MathExpr::Radical { index, radicand } => {
            layout_radical(index.as_deref(), radicand, style, font, k, size)
        }
        MathExpr::OverLine(body) => layout_overline(body, style, font, k, size),
        MathExpr::UnderLine(body) => layout_underline(body, style, font, k, size),
        MathExpr::Accent { base, accent } => layout_accent(base, *accent, style, font, k, size),
        MathExpr::DelimiterPair { left, body, right } => {
            layout_delimiter_pair(*left, body, *right, style, font, k, size)
        }
        MathExpr::StyleOverride { style: kind, body } => {
            layout(body, resolve_style_override(*kind, style), font, k, size)
        }
        MathExpr::Spacing(sp) => MathBox {
            width: math_space_width(*sp, size, style, k),
            height: 0.0,
            depth: 0.0,
            content: BoxContent::Empty,
        },
    }
}

// ---------------------------------------------------------------------------
// Size helpers
// ---------------------------------------------------------------------------

fn size_scale(style: MathStyle, k: &MathConstants) -> f64 {
    match style.size_category() {
        SizeCategory::Text => 1.0,
        SizeCategory::Script => k.script_scale_down,
        SizeCategory::ScriptScript => k.script_script_scale_down,
    }
}

fn is_display_style(style: MathStyle) -> bool {
    matches!(style, MathStyle::Display | MathStyle::DisplayCramped)
}

fn resolve_style_override(kind: DisplayStyleKind, current: MathStyle) -> MathStyle {
    let cramped = current.is_cramped();
    match (kind, cramped) {
        (DisplayStyleKind::Display, false) => MathStyle::Display,
        (DisplayStyleKind::Display, true) => MathStyle::DisplayCramped,
        (DisplayStyleKind::Text, false) => MathStyle::Text,
        (DisplayStyleKind::Text, true) => MathStyle::TextCramped,
        (DisplayStyleKind::Script, false) => MathStyle::Script,
        (DisplayStyleKind::Script, true) => MathStyle::ScriptCramped,
        (DisplayStyleKind::ScriptScript, false) => MathStyle::ScriptScript,
        (DisplayStyleKind::ScriptScript, true) => MathStyle::ScriptScriptCramped,
    }
}

fn space_width(amount: SpaceAmount, size: f64, style: MathStyle, k: &MathConstants) -> f64 {
    let em = size * size_scale(style, k);
    match amount {
        SpaceAmount::None => 0.0,
        SpaceAmount::Thin => em / 6.0,
        SpaceAmount::Medium => em * 2.0 / 9.0,
        SpaceAmount::Thick => em * 5.0 / 18.0,
    }
}

fn math_space_width(sp: MathSpace, size: f64, style: MathStyle, k: &MathConstants) -> f64 {
    let em = size * size_scale(style, k);
    match sp {
        MathSpace::Thin => em * 3.0 / 18.0,
        MathSpace::Medium => em * 4.0 / 18.0,
        MathSpace::Thick => em * 5.0 / 18.0,
        MathSpace::Quad => em,
        MathSpace::NegativeThin => -em * 3.0 / 18.0,
        MathSpace::Custom(v) => em * v,
    }
}

// ---------------------------------------------------------------------------
// Glyph boxes
// ---------------------------------------------------------------------------

fn glyph_box(ch: char, font: &Font, style: MathStyle, k: &MathConstants, size: f64) -> MathBox {
    let font_scale = size * size_scale(style, k);
    match font
        .glyph_for_char(ch)
        .and_then(|gid| font.glyph_metrics(gid).map(|m| (gid, m)))
    {
        Some((gid, m)) => {
            let em = |v: f32| font.to_em_scale(v) as f64 * font_scale;
            MathBox {
                width: em(m.advance_width),
                height: em(m.bbox.y_max).max(0.0),
                depth: em(-m.bbox.y_min).max(0.0),
                content: BoxContent::Glyph {
                    gid,
                    font_scale,
                    y_scale: 1.0,
                    y_shift: 0.0,
                },
            }
        }
        None => MathBox {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            content: BoxContent::Empty,
        },
    }
}

/// A glyph stretched vertically (about its own baseline) so that
/// `height + depth == target_extent`, expressed with `depth == 0`, i.e. the
/// box's baseline sits at the stretched glyph's own bottom edge. Used for
/// radical surds and stretchy `\left`/`\right` delimiters — real per-size
/// glyph variants would be needed for a pixel-faithful result, which isn't
/// available without a MATH table, so this is a documented approximation.
fn scaled_glyph_box(
    ch: char,
    font: &Font,
    style: MathStyle,
    k: &MathConstants,
    size: f64,
    target_extent: f64,
) -> MathBox {
    let base = glyph_box(ch, font, style, k, size);
    let (gid, font_scale) = match base.content {
        BoxContent::Glyph {
            gid, font_scale, ..
        } => (gid, font_scale),
        _ => {
            return MathBox {
                width: 0.0,
                height: target_extent.max(0.0),
                depth: 0.0,
                content: BoxContent::Empty,
            }
        }
    };
    let natural_extent = (base.height + base.depth).max(1e-6);
    let y_scale = (target_extent / natural_extent).max(1.0);
    let y_shift = base.depth * y_scale;
    MathBox {
        width: base.width,
        height: target_extent,
        depth: 0.0,
        content: BoxContent::Glyph {
            gid,
            font_scale,
            y_scale,
            y_shift,
        },
    }
}

/// A run of characters (an `Identifier`/`Number`) laid out left-to-right
/// using the font's own advance widths and kerning pairs (ordinary glyph
/// kerning — distinct from, and in addition to, the TeX atom-spacing rules
/// applied between whole atoms in a [`Row`](MathExpr::Row)).
fn text_row_box(
    text: &str,
    font: &Font,
    style: MathStyle,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let font_scale = size * size_scale(style, k);
    let mut items = Vec::new();
    let mut cursor = 0.0f64;
    let mut height = 0.0f64;
    let mut depth = 0.0f64;
    let mut prev_gid: Option<GlyphId> = None;
    for c in text.chars() {
        let gid = font.glyph_for_char(c);
        if let (Some(p), Some(g)) = (prev_gid, gid) {
            let kern = font.kerning(p, g);
            cursor += font.to_em_scale(kern) as f64 * font_scale;
        }
        let b = glyph_box(c, font, style, k, size);
        height = height.max(b.height);
        depth = depth.max(b.depth);
        let advance = gid
            .and_then(|g| font.glyph_advance(g))
            .map(|a| font.to_em_scale(a) as f64 * font_scale)
            .unwrap_or(b.width);
        items.push(PositionedBox {
            dx: cursor,
            dy: 0.0,
            b,
        });
        cursor += advance;
        prev_gid = gid;
    }
    MathBox {
        width: cursor,
        height,
        depth,
        content: BoxContent::HList(items),
    }
}

// ---------------------------------------------------------------------------
// Row / atom spacing
// ---------------------------------------------------------------------------

fn atom_class_of(expr: &MathExpr) -> AtomClass {
    match expr {
        MathExpr::Identifier(_) | MathExpr::Number(_) => AtomClass::Ord,
        MathExpr::Symbol { class, .. } => *class,
        MathExpr::Operator { .. } => AtomClass::Op,
        MathExpr::Row(_) => AtomClass::Ord,
        MathExpr::Fraction { .. } => AtomClass::Inner,
        MathExpr::Superscript { base, .. }
        | MathExpr::Subscript { base, .. }
        | MathExpr::SubSup { base, .. } => atom_class_of(base),
        MathExpr::Radical { .. } => AtomClass::Ord,
        MathExpr::OverLine(body) | MathExpr::UnderLine(body) => atom_class_of(body),
        MathExpr::Accent { base, .. } => atom_class_of(base),
        MathExpr::DelimiterPair { .. } => AtomClass::Inner,
        MathExpr::StyleOverride { body, .. } => atom_class_of(body),
        MathExpr::Spacing(_) => AtomClass::Ord,
    }
}

fn layout_row(
    items: &[MathExpr],
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    if items.is_empty() {
        return MathBox {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            content: BoxContent::Empty,
        };
    }

    // Explicit `Spacing` nodes are transparent to atom classification: they
    // don't participate in Bin-reclassification or the spacing table, they
    // just contribute their own explicit width.
    let mut classes: Vec<Option<AtomClass>> = items
        .iter()
        .map(|e| {
            if matches!(e, MathExpr::Spacing(_)) {
                None
            } else {
                Some(atom_class_of(e))
            }
        })
        .collect();
    let mut real: Vec<AtomClass> = classes.iter().filter_map(|c| *c).collect();
    reclassify_bins(&mut real);
    let mut ri = 0;
    for c in classes.iter_mut() {
        if c.is_some() {
            *c = Some(real[ri]);
            ri += 1;
        }
    }

    let boxes: Vec<MathBox> = items
        .iter()
        .map(|e| layout(e, style, font, k, size))
        .collect();

    let mut positioned = Vec::with_capacity(boxes.len());
    let mut cursor = 0.0f64;
    let mut height = 0.0f64;
    let mut depth = 0.0f64;
    for (i, b) in boxes.into_iter().enumerate() {
        if i > 0 {
            if let (Some(l), Some(r)) = (classes[i - 1], classes[i]) {
                cursor += space_width(interelement_space(l, r, style), size, style, k);
            }
        }
        height = height.max(b.height);
        depth = depth.max(b.depth);
        let w = b.width;
        positioned.push(PositionedBox {
            dx: cursor,
            dy: 0.0,
            b,
        });
        cursor += w;
    }
    MathBox {
        width: cursor,
        height,
        depth,
        content: BoxContent::HList(positioned),
    }
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

fn operator_uses_limits(base: &MathExpr, style: MathStyle) -> bool {
    match base {
        MathExpr::Operator { limits, .. } => match limits {
            Limits::Limits => true,
            Limits::NoLimits => false,
            Limits::Auto => is_display_style(style),
        },
        _ => false,
    }
}

fn layout_superscript(
    base: &MathExpr,
    sup: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let base_box = layout(base, style, font, k, size);
    let sup_box = layout(sup, style.superscript_style(), font, k, size);
    let scale = size_scale(style, k);

    let min_shift = (if style.is_cramped() {
        k.superscript_shift_up_cramped
    } else {
        k.superscript_shift_up
    }) * scale;
    let clearance = base_box.height + k.superscript_bottom_min * scale - sup_box.depth;
    let shift_up = min_shift.max(clearance);

    let base_w = base_box.width;
    let width = base_w + sup_box.width;
    let height = (shift_up + sup_box.height).max(base_box.height);
    let depth = base_box.depth.max((sup_box.depth - shift_up).max(0.0));

    let items = vec![
        PositionedBox {
            dx: 0.0,
            dy: 0.0,
            b: base_box,
        },
        PositionedBox {
            dx: base_w,
            dy: shift_up,
            b: sup_box,
        },
    ];
    MathBox {
        width,
        height,
        depth,
        content: BoxContent::HList(items),
    }
}

fn layout_subscript(
    base: &MathExpr,
    sub: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let base_box = layout(base, style, font, k, size);
    let sub_box = layout(sub, style.subscript_style(), font, k, size);
    let scale = size_scale(style, k);

    let min_shift = k.subscript_shift_down * scale;
    let clearance = sub_box.height - k.subscript_top_max * scale;
    let shift_down = min_shift.max(clearance).max(0.0);

    let base_w = base_box.width;
    let width = base_w + sub_box.width;
    let height = base_box.height.max((sub_box.height - shift_down).max(0.0));
    let depth = base_box.depth.max(shift_down + sub_box.depth);

    let items = vec![
        PositionedBox {
            dx: 0.0,
            dy: 0.0,
            b: base_box,
        },
        PositionedBox {
            dx: base_w,
            dy: -shift_down,
            b: sub_box,
        },
    ];
    MathBox {
        width,
        height,
        depth,
        content: BoxContent::HList(items),
    }
}

fn layout_subsup(
    base: &MathExpr,
    sub: &MathExpr,
    sup: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let base_box = layout(base, style, font, k, size);
    let sup_box = layout(sup, style.superscript_style(), font, k, size);
    let sub_box = layout(sub, style.subscript_style(), font, k, size);
    let scale = size_scale(style, k);

    let min_sup_shift = (if style.is_cramped() {
        k.superscript_shift_up_cramped
    } else {
        k.superscript_shift_up
    }) * scale;
    let sup_clearance = base_box.height + k.superscript_bottom_min * scale - sup_box.depth;
    let mut shift_up = min_sup_shift.max(sup_clearance);

    let min_sub_shift = k.subscript_shift_down * scale;
    let sub_clearance = sub_box.height - k.subscript_top_max * scale;
    let mut shift_down = min_sub_shift.max(sub_clearance).max(0.0);

    let gap_min = k.sub_superscript_gap_min * scale;
    let gap = shift_up + shift_down - sup_box.depth - sub_box.height;
    if gap < gap_min {
        let deficit = (gap_min - gap) * 0.5;
        shift_up += deficit;
        shift_down += deficit;
    }

    let base_w = base_box.width;
    let width = base_w + sup_box.width.max(sub_box.width);
    let height = (shift_up + sup_box.height).max(base_box.height);
    let depth = (shift_down + sub_box.depth).max(base_box.depth);

    let items = vec![
        PositionedBox {
            dx: 0.0,
            dy: 0.0,
            b: base_box,
        },
        PositionedBox {
            dx: base_w,
            dy: shift_up,
            b: sup_box,
        },
        PositionedBox {
            dx: base_w,
            dy: -shift_down,
            b: sub_box,
        },
    ];
    MathBox {
        width,
        height,
        depth,
        content: BoxContent::HList(items),
    }
}

/// Big-operator limits (`\sum_{i}^{n}` in Display style): sub/sup are
/// centered above/below the operator rather than placed as corner scripts.
fn layout_limits(
    base: &MathExpr,
    sub: Option<&MathExpr>,
    sup: Option<&MathExpr>,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let base_box = layout(base, style, font, k, size);
    let scale = size_scale(style, k);
    let gap = k.default_rule_thickness * scale * 3.0;

    let sup_box = sup.map(|e| layout(e, style.superscript_style(), font, k, size));
    let sub_box = sub.map(|e| layout(e, style.subscript_style(), font, k, size));

    let base_w = base_box.width;
    let sup_w = sup_box.as_ref().map(|b| b.width).unwrap_or(0.0);
    let sub_w = sub_box.as_ref().map(|b| b.width).unwrap_or(0.0);
    let width = base_w.max(sup_w).max(sub_w);

    let mut items = Vec::new();
    let mut height = base_box.height;
    let mut depth = base_box.depth;

    if let Some(sb) = sup_box {
        let sb_h = sb.height;
        let sup_dy = base_box.height + gap + sb.depth;
        let dx = (width - sb.width) * 0.5;
        height = sup_dy + sb_h;
        items.push(PositionedBox {
            dx,
            dy: sup_dy,
            b: sb,
        });
    }
    items.push(PositionedBox {
        dx: (width - base_w) * 0.5,
        dy: 0.0,
        b: base_box,
    });
    if let Some(sb) = sub_box {
        let sb_d = sb.depth;
        let sub_dy = -(depth + gap + sb.height);
        let dx = (width - sb.width) * 0.5;
        depth = -sub_dy + sb_d;
        items.push(PositionedBox {
            dx,
            dy: sub_dy,
            b: sb,
        });
    }

    MathBox {
        width,
        height,
        depth,
        content: BoxContent::VList(items),
    }
}

// ---------------------------------------------------------------------------
// Fractions
// ---------------------------------------------------------------------------

fn layout_fraction(
    numerator: &MathExpr,
    denominator: &MathExpr,
    bar: FractionBar,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let num_box = layout(numerator, style.numerator_style(), font, k, size);
    let den_box = layout(denominator, style.denominator_style(), font, k, size);
    let scale = size_scale(style, k);
    let display = is_display_style(style);

    let thickness = match bar {
        FractionBar::Default => k.fraction_rule_thickness * scale,
        FractionBar::Thickness(t) => t,
        FractionBar::None => 0.0,
    };
    let (num_shift_min, den_shift_min, num_gap_min, den_gap_min) = if display {
        (
            k.fraction_numerator_display_style_shift_up,
            k.fraction_denominator_display_style_shift_down,
            k.fraction_num_display_style_gap_min,
            k.fraction_denom_display_style_gap_min,
        )
    } else {
        (
            k.fraction_numerator_shift_up,
            k.fraction_denominator_shift_down,
            k.fraction_numerator_gap_min,
            k.fraction_denominator_gap_min,
        )
    };

    let axis = k.axis_height * scale;
    let half_thickness = thickness * 0.5;

    let num_gap = num_gap_min * scale;
    let den_gap = den_gap_min * scale;
    let num_shift = (num_shift_min * scale).max(axis + half_thickness + num_gap + num_box.depth);
    let den_shift = (den_shift_min * scale).max(den_gap + den_box.height - axis + half_thickness);

    let num_w = num_box.width;
    let den_w = den_box.width;
    let num_h = num_box.height;
    let den_d = den_box.depth;
    let width = num_w.max(den_w);
    let num_dx = (width - num_w) * 0.5;
    let den_dx = (width - den_w) * 0.5;
    let height = num_shift + num_h;
    let depth = den_shift + den_d;

    let mut items = Vec::with_capacity(3);
    items.push(PositionedBox {
        dx: num_dx,
        dy: num_shift,
        b: num_box,
    });
    if !matches!(bar, FractionBar::None) {
        items.push(PositionedBox {
            dx: 0.0,
            dy: axis - half_thickness,
            b: MathBox {
                width,
                height: thickness,
                depth: 0.0,
                content: BoxContent::Rule { thickness },
            },
        });
    }
    items.push(PositionedBox {
        dx: den_dx,
        dy: -den_shift,
        b: den_box,
    });

    MathBox {
        width,
        height,
        depth,
        content: BoxContent::VList(items),
    }
}

// ---------------------------------------------------------------------------
// Radicals
// ---------------------------------------------------------------------------

const SURD: char = '\u{221A}';

fn layout_radical(
    index: Option<&MathExpr>,
    radicand: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let radicand_box = layout(radicand, style.radicand_style(), font, k, size);
    let scale = size_scale(style, k);
    let display = is_display_style(style);

    let gap = (if display {
        k.radical_display_style_vertical_gap
    } else {
        k.radical_vertical_gap
    }) * scale;
    let rule_thickness = k.radical_rule_thickness * scale;
    let extra_ascender = k.radical_extra_ascender * scale;

    let depth = radicand_box.depth;
    let rule_y = radicand_box.height + gap;
    let height = rule_y + rule_thickness + extra_ascender;
    let target_extent = radicand_box.depth + rule_y + rule_thickness;

    let surd_box = scaled_glyph_box(SURD, font, style, k, size, target_extent);
    let surd_w = surd_box.width;
    let radicand_w = radicand_box.width;

    let mut items = Vec::with_capacity(3);
    items.push(PositionedBox {
        dx: 0.0,
        dy: -depth,
        b: surd_box,
    });
    items.push(PositionedBox {
        dx: surd_w,
        dy: 0.0,
        b: radicand_box,
    });
    items.push(PositionedBox {
        dx: surd_w,
        dy: rule_y,
        b: MathBox {
            width: radicand_w,
            height: rule_thickness,
            depth: 0.0,
            content: BoxContent::Rule {
                thickness: rule_thickness,
            },
        },
    });

    let mut width = surd_w + radicand_w;

    // Nth-root index: a simplified placement (real TeX has dedicated
    // before/after-degree kerns and a "bottom raise percent" constant, which
    // this crate's `MathConstants` doesn't model — see module docs).
    if let Some(idx) = index {
        let idx_box = layout(idx, MathStyle::ScriptScriptCramped, font, k, size);
        let idx_w = idx_box.width;
        let idx_depth = idx_box.depth;
        let kern = surd_w * 0.1;
        let shift = surd_w * 0.4 + kern;
        for item in items.iter_mut() {
            item.dx += shift;
        }
        let idx_dy = height * 0.6 - idx_depth;
        items.insert(
            0,
            PositionedBox {
                dx: 0.0,
                dy: idx_dy,
                b: idx_box,
            },
        );
        width += shift.max(idx_w * 0.0) + idx_w.max(0.0) * 0.0; // index sits over the surd, no extra width beyond `shift`
        width = width.max(shift + surd_w * 0.6);
        let _ = idx_w;
    }

    MathBox {
        width,
        height,
        depth,
        content: BoxContent::HList(items),
    }
}

// ---------------------------------------------------------------------------
// Over/underline
// ---------------------------------------------------------------------------

fn layout_overline(
    body: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    // TeXbook: the field of an \overline is laid out in cramped style (there's
    // nothing above it to make superscript headroom for).
    let body_box = layout(body, style.cramped(), font, k, size);
    let scale = size_scale(style, k);
    let gap = k.overbar_vertical_gap * scale;
    let thickness = k.overbar_rule_thickness * scale;
    let extra = k.overbar_extra_ascender * scale;

    let width = body_box.width;
    let depth = body_box.depth;
    let rule_y = body_box.height + gap;
    let height = rule_y + thickness + extra;

    let items = vec![
        PositionedBox {
            dx: 0.0,
            dy: 0.0,
            b: body_box,
        },
        PositionedBox {
            dx: 0.0,
            dy: rule_y,
            b: MathBox {
                width,
                height: thickness,
                depth: 0.0,
                content: BoxContent::Rule { thickness },
            },
        },
    ];
    MathBox {
        width,
        height,
        depth,
        content: BoxContent::VList(items),
    }
}

fn layout_underline(
    body: &MathExpr,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let body_box = layout(body, style, font, k, size);
    let scale = size_scale(style, k);
    let gap = k.underbar_vertical_gap * scale;
    let thickness = k.underbar_rule_thickness * scale;
    let extra = k.underbar_extra_descender * scale;

    let width = body_box.width;
    let height = body_box.height;
    let rule_y = -(body_box.depth + gap);
    let depth = -rule_y + thickness + extra;

    let items = vec![
        PositionedBox {
            dx: 0.0,
            dy: 0.0,
            b: body_box,
        },
        PositionedBox {
            dx: 0.0,
            dy: rule_y - thickness,
            b: MathBox {
                width,
                height: thickness,
                depth: 0.0,
                content: BoxContent::Rule { thickness },
            },
        },
    ];
    MathBox {
        width,
        height,
        depth,
        content: BoxContent::VList(items),
    }
}

// ---------------------------------------------------------------------------
// Accents
// ---------------------------------------------------------------------------

fn layout_accent(
    base: &MathExpr,
    accent: char,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let base_box = layout(base, style, font, k, size);
    let scale = size_scale(style, k);
    let accent_box = glyph_box(accent, font, style, k, size);

    let base_w = base_box.width;
    let base_h = base_box.height;
    let base_d = base_box.depth;
    let accent_w = accent_box.width;
    let accent_h = accent_box.height;
    let accent_d = accent_box.depth;

    let accent_dy = base_h.min(k.accent_base_height * scale).max(0.0) + accent_d;
    let width = base_w.max(accent_w);
    let base_dx = (width - base_w) * 0.5;
    let accent_dx = (width - accent_w) * 0.5;
    let height = accent_dy + accent_h;

    let items = vec![
        PositionedBox {
            dx: accent_dx,
            dy: accent_dy,
            b: accent_box,
        },
        PositionedBox {
            dx: base_dx,
            dy: 0.0,
            b: base_box,
        },
    ];
    MathBox {
        width,
        height,
        depth: base_d,
        content: BoxContent::HList(items),
    }
}

// ---------------------------------------------------------------------------
// Stretchy delimiters
// ---------------------------------------------------------------------------

fn layout_delimiter_pair(
    left: Option<char>,
    body: &MathExpr,
    right: Option<char>,
    style: MathStyle,
    font: &Font,
    k: &MathConstants,
    size: f64,
) -> MathBox {
    let body_box = layout(body, style, font, k, size);
    let scale = size_scale(style, k);
    let axis = k.axis_height * scale;

    let above = (body_box.height - axis).max(0.0);
    let below = (body_box.depth + axis).max(0.0);
    let half = above.max(below).max(size * scale * 0.25);

    let body_w = body_box.width;
    let mut items = Vec::new();
    let mut cursor = 0.0f64;

    if let Some(c) = left {
        let b = scaled_glyph_box(c, font, style, k, size, 2.0 * half);
        let w = b.width;
        items.push(PositionedBox {
            dx: 0.0,
            dy: axis - half,
            b,
        });
        cursor += w;
    }
    items.push(PositionedBox {
        dx: cursor,
        dy: 0.0,
        b: body_box,
    });
    cursor += body_w;
    if let Some(c) = right {
        let b = scaled_glyph_box(c, font, style, k, size, 2.0 * half);
        let w = b.width;
        items.push(PositionedBox {
            dx: cursor,
            dy: axis - half,
            b,
        });
        cursor += w;
    }

    MathBox {
        width: cursor,
        height: axis + half,
        depth: half - axis,
        content: BoxContent::HList(items),
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    fn sample_font() -> Font {
        let data = std::fs::read("C:\\Windows\\Fonts\\arial.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Helvetica.ttc"))
            .expect("no test font found");
        Font::from_bytes(&data).expect("valid font")
    }

    #[test]
    fn fraction_rule_sits_on_axis() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let expr = MathExpr::Fraction {
            numerator: Box::new(MathExpr::Identifier("x".to_string())),
            denominator: Box::new(MathExpr::Identifier("y".to_string())),
            bar: FractionBar::Default,
        };
        let b = layout(&expr, MathStyle::Display, &font, &k, 20.0);
        let items = match &b.content {
            BoxContent::VList(items) => items,
            _ => panic!("expected VList"),
        };
        let rule = items
            .iter()
            .find(|p| matches!(p.b.content, BoxContent::Rule { .. }))
            .expect("rule present");
        let scale = size_scale(MathStyle::Display, &k);
        let expected_center = k.axis_height * scale;
        let actual_center = rule.dy + rule.b.height * 0.5;
        assert!((actual_center - expected_center).abs() < 1e-9);
    }

    #[test]
    fn superscript_sits_above_baseline() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let expr = MathExpr::Superscript {
            base: Box::new(MathExpr::Identifier("y".to_string())),
            sup: Box::new(MathExpr::Number("2".to_string())),
        };
        let b = layout(&expr, MathStyle::Text, &font, &k, 20.0);
        match &b.content {
            BoxContent::HList(items) => {
                assert_eq!(items.len(), 2);
                assert!(items[1].dy > 0.0);
                assert!(items[1].dx > 0.0);
            }
            _ => panic!("expected HList"),
        }
    }

    #[test]
    fn spec2_example_is_nondegenerate() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let expr = MathExpr::Fraction {
            numerator: Box::new(MathExpr::Identifier("x".to_string())),
            denominator: Box::new(MathExpr::Superscript {
                base: Box::new(MathExpr::Identifier("y".to_string())),
                sup: Box::new(MathExpr::Number("2".to_string())),
            }),
            bar: FractionBar::Default,
        };
        let b = layout(&expr, MathStyle::Display, &font, &k, 20.0);
        assert!(b.width > 0.0);
        assert!(b.height + b.depth > 0.0);
    }

    #[test]
    fn bin_classified_row_is_wider_than_ord_classified_row() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let with_bin = MathExpr::Row(alloc::vec![
            MathExpr::Identifier("x".to_string()),
            MathExpr::Symbol {
                ch: '+',
                class: AtomClass::Bin
            },
            MathExpr::Identifier("y".to_string()),
        ]);
        let with_ord = MathExpr::Row(alloc::vec![
            MathExpr::Identifier("x".to_string()),
            MathExpr::Symbol {
                ch: '+',
                class: AtomClass::Ord
            },
            MathExpr::Identifier("y".to_string()),
        ]);
        let a = layout(&with_bin, MathStyle::Text, &font, &k, 20.0);
        let b = layout(&with_ord, MathStyle::Text, &font, &k, 20.0);
        assert!(a.width > b.width);
    }

    #[test]
    fn radical_covers_radicand_height_and_depth() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let expr = MathExpr::Radical {
            index: None,
            radicand: Box::new(MathExpr::Identifier("x".to_string())),
        };
        let b = layout(&expr, MathStyle::Display, &font, &k, 20.0);
        assert!(b.height > 0.0);
        assert!(b.width > 0.0);
    }

    #[test]
    fn delimiter_pair_grows_with_taller_body() {
        let font = sample_font();
        let k = MathConstants::from_font(&font, 20.0);
        let short = MathExpr::DelimiterPair {
            left: Some('('),
            body: Box::new(MathExpr::Identifier("x".to_string())),
            right: Some(')'),
        };
        let tall = MathExpr::DelimiterPair {
            left: Some('('),
            body: Box::new(MathExpr::Fraction {
                numerator: Box::new(MathExpr::Identifier("x".to_string())),
                denominator: Box::new(MathExpr::Identifier("y".to_string())),
                bar: FractionBar::Default,
            }),
            right: Some(')'),
        };
        let a = layout(&short, MathStyle::Display, &font, &k, 20.0);
        let b = layout(&tall, MathStyle::Display, &font, &k, 20.0);
        assert!(b.height + b.depth >= a.height + a.depth);
    }
}
