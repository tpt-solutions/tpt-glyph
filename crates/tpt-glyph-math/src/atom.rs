// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/atom
//
// The eight TeX math atom classes and the TeXbook Appendix G interelement
// spacing table, plus the Bin-to-Ord reclassification pass (TeXbook Rule 17)
// that must run over a row before the spacing table is consulted.

use crate::style::{MathStyle, SizeCategory};

/// A TeX math atom classification (TeXbook Chapter 17 / Appendix G).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AtomClass {
    /// Ordinary atom: variables, most symbols.
    Ord,
    /// Large operator: `\sum`, `\int`, `\prod`, ...
    Op,
    /// Binary operator: `+`, `-`, `\times`, ...
    Bin,
    /// Relation: `=`, `<`, `\leq`, ...
    Rel,
    /// Opening delimiter: `(`, `[`, ...
    Open,
    /// Closing delimiter: `)`, `]`, ...
    Close,
    /// Punctuation: `,`, `;`, ...
    Punct,
    /// A sub-formula treated as a single unit (e.g. a fraction).
    Inner,
}

impl AtomClass {
    fn index(self) -> usize {
        match self {
            AtomClass::Ord => 0,
            AtomClass::Op => 1,
            AtomClass::Bin => 2,
            AtomClass::Rel => 3,
            AtomClass::Open => 4,
            AtomClass::Close => 5,
            AtomClass::Punct => 6,
            AtomClass::Inner => 7,
        }
    }
}

/// The amount of interelement glue the spacing table prescribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceAmount {
    None,
    Thin,
    Medium,
    Thick,
}

/// One spacing-table cell: the base amount, and whether it is suppressed
/// (TeXbook: "starred" entries) outside of Display/Text style.
#[derive(Debug, Clone, Copy)]
struct Cell(SpaceAmount, bool);

const N: Cell = Cell(SpaceAmount::None, false);
const T: Cell = Cell(SpaceAmount::Thin, false);
const TS: Cell = Cell(SpaceAmount::Thin, true);
const MS: Cell = Cell(SpaceAmount::Medium, true);
const KS: Cell = Cell(SpaceAmount::Thick, true);

/// TeXbook Appendix G interelement spacing table, indexed `[left][right]`.
/// Combinations that cannot occur after Bin-reclassification (e.g.
/// `Bin`-`Bin`) are given a `None` entry since real input never reaches them.
const SPACING: [[Cell; 8]; 8] = [
    // Ord
    [N, T, MS, KS, N, N, N, TS],
    // Op
    [T, T, N, KS, N, N, N, TS],
    // Bin
    [MS, MS, N, N, MS, N, N, MS],
    // Rel
    [KS, KS, N, N, KS, N, N, KS],
    // Open
    [N, N, N, N, N, N, N, N],
    // Close
    [N, T, MS, KS, N, N, N, TS],
    // Punct
    [TS, TS, N, TS, TS, TS, TS, TS],
    // Inner
    [TS, T, MS, KS, TS, N, TS, TS],
];

/// Look up the interelement spacing between two adjacent atoms in `style`.
///
/// Medium/thick entries (the TeXbook's "starred" cells) are suppressed to
/// `None` in Script/ScriptScript style; thin spacing is unaffected.
pub fn interelement_space(left: AtomClass, right: AtomClass, style: MathStyle) -> SpaceAmount {
    let Cell(amount, starred) = SPACING[left.index()][right.index()];
    if starred && style.size_category() != SizeCategory::Text {
        SpaceAmount::None
    } else {
        amount
    }
}

/// Apply TeXbook Rule 17 in place: a `Bin` atom becomes `Ord` if it is the
/// first atom of the row, the last atom of the row, or immediately follows
/// a `Bin`, `Op`, `Rel`, `Open`, or `Punct` atom.
pub fn reclassify_bins(classes: &mut [AtomClass]) {
    if classes.is_empty() {
        return;
    }
    let mut prev: Option<AtomClass> = None;
    for c in classes.iter_mut() {
        if *c == AtomClass::Bin {
            let demote = matches!(
                prev,
                None | Some(
                    AtomClass::Bin
                        | AtomClass::Op
                        | AtomClass::Rel
                        | AtomClass::Open
                        | AtomClass::Punct
                )
            );
            if demote {
                *c = AtomClass::Ord;
            }
        }
        prev = Some(*c);
    }
    if *classes.last().unwrap() == AtomClass::Bin {
        *classes.last_mut().unwrap() = AtomClass::Ord;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ord_ord_is_no_space() {
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Ord, MathStyle::Text),
            SpaceAmount::None
        );
    }

    #[test]
    fn ord_op_is_thin_in_text_and_script() {
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Op, MathStyle::Text),
            SpaceAmount::Thin
        );
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Op, MathStyle::Script),
            SpaceAmount::Thin
        );
    }

    #[test]
    fn ord_bin_is_medium_in_text_but_suppressed_in_script() {
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Bin, MathStyle::Text),
            SpaceAmount::Medium
        );
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Bin, MathStyle::Script),
            SpaceAmount::None
        );
        assert_eq!(
            interelement_space(AtomClass::Ord, AtomClass::Bin, MathStyle::ScriptScript),
            SpaceAmount::None
        );
    }

    #[test]
    fn rel_rel_is_no_space() {
        assert_eq!(
            interelement_space(AtomClass::Rel, AtomClass::Rel, MathStyle::Display),
            SpaceAmount::None
        );
    }

    #[test]
    fn leading_bin_becomes_ord() {
        let mut classes = [AtomClass::Bin, AtomClass::Ord];
        reclassify_bins(&mut classes);
        assert_eq!(classes, [AtomClass::Ord, AtomClass::Ord]);
    }

    #[test]
    fn bin_after_rel_becomes_ord_unary_minus() {
        // "x < -y": Rel, Bin, Ord -> the Bin (unary minus) becomes Ord.
        let mut classes = [
            AtomClass::Ord,
            AtomClass::Rel,
            AtomClass::Bin,
            AtomClass::Ord,
        ];
        reclassify_bins(&mut classes);
        assert_eq!(
            classes,
            [
                AtomClass::Ord,
                AtomClass::Rel,
                AtomClass::Ord,
                AtomClass::Ord
            ]
        );
    }

    #[test]
    fn bin_between_ords_stays_bin() {
        let mut classes = [AtomClass::Ord, AtomClass::Bin, AtomClass::Ord];
        reclassify_bins(&mut classes);
        assert_eq!(classes, [AtomClass::Ord, AtomClass::Bin, AtomClass::Ord]);
    }

    #[test]
    fn trailing_bin_becomes_ord() {
        let mut classes = [AtomClass::Ord, AtomClass::Bin];
        reclassify_bins(&mut classes);
        assert_eq!(classes, [AtomClass::Ord, AtomClass::Ord]);
    }
}
