// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-math/style
//
// The eight TeX math styles (Display/Text/Script/ScriptScript, each with a
// cramped variant) and the style-transition rules TeXbook Chapter 17 uses
// when descending into superscripts, subscripts, fraction numerators and
// denominators, and radicands.

/// A TeX math layout style. "Cramped" styles suppress the extra headroom
/// normally reserved for superscripts (used e.g. for fraction denominators
/// and radicands, where there's nothing above the baseline to make room for).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MathStyle {
    Display,
    DisplayCramped,
    Text,
    TextCramped,
    Script,
    ScriptCramped,
    ScriptScript,
    ScriptScriptCramped,
}

/// Which of the (at most) three physical sizes a style renders at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeCategory {
    Text,
    Script,
    ScriptScript,
}

impl MathStyle {
    /// The cramped variant of this style (idempotent if already cramped).
    pub fn cramped(self) -> Self {
        match self {
            MathStyle::Display | MathStyle::DisplayCramped => MathStyle::DisplayCramped,
            MathStyle::Text | MathStyle::TextCramped => MathStyle::TextCramped,
            MathStyle::Script | MathStyle::ScriptCramped => MathStyle::ScriptCramped,
            MathStyle::ScriptScript | MathStyle::ScriptScriptCramped => {
                MathStyle::ScriptScriptCramped
            }
        }
    }

    pub fn is_cramped(self) -> bool {
        matches!(
            self,
            MathStyle::DisplayCramped
                | MathStyle::TextCramped
                | MathStyle::ScriptCramped
                | MathStyle::ScriptScriptCramped
        )
    }

    pub fn size_category(self) -> SizeCategory {
        match self {
            MathStyle::Display
            | MathStyle::DisplayCramped
            | MathStyle::Text
            | MathStyle::TextCramped => SizeCategory::Text,
            MathStyle::Script | MathStyle::ScriptCramped => SizeCategory::Script,
            MathStyle::ScriptScript | MathStyle::ScriptScriptCramped => SizeCategory::ScriptScript,
        }
    }

    /// One step down the D > T > S > SS size ladder, preserving crampedness.
    fn one_smaller(self) -> Self {
        match self {
            MathStyle::Display => MathStyle::Text,
            MathStyle::DisplayCramped => MathStyle::TextCramped,
            MathStyle::Text => MathStyle::Script,
            MathStyle::TextCramped => MathStyle::ScriptCramped,
            MathStyle::Script => MathStyle::ScriptScript,
            MathStyle::ScriptCramped => MathStyle::ScriptScriptCramped,
            MathStyle::ScriptScript => MathStyle::ScriptScript,
            MathStyle::ScriptScriptCramped => MathStyle::ScriptScriptCramped,
        }
    }

    /// The style used to lay out a superscript: Display/Text collapse to
    /// Script directly (exponents are conventionally script-sized even in
    /// text style), Script goes to ScriptScript, ScriptScript stays put.
    pub fn superscript_style(self) -> Self {
        match self {
            MathStyle::Display | MathStyle::Text => MathStyle::Script,
            MathStyle::DisplayCramped | MathStyle::TextCramped => MathStyle::ScriptCramped,
            MathStyle::Script => MathStyle::ScriptScript,
            MathStyle::ScriptCramped => MathStyle::ScriptScriptCramped,
            MathStyle::ScriptScript => MathStyle::ScriptScript,
            MathStyle::ScriptScriptCramped => MathStyle::ScriptScriptCramped,
        }
    }

    /// The style used to lay out a subscript: always the cramped version of
    /// the superscript style (subscripts never get extra headroom).
    pub fn subscript_style(self) -> Self {
        self.superscript_style().cramped()
    }

    /// The style used to lay out a fraction numerator: one step down the
    /// size ladder, preserving crampedness.
    pub fn numerator_style(self) -> Self {
        self.one_smaller()
    }

    /// The style used to lay out a fraction denominator: one step down the
    /// size ladder, always cramped.
    pub fn denominator_style(self) -> Self {
        self.one_smaller().cramped()
    }

    /// The style used to lay out a radicand: same size, always cramped
    /// (a radical changes the size of the surd sign to fit its content, not
    /// the content's own style).
    pub fn radicand_style(self) -> Self {
        self.cramped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cramped_is_idempotent() {
        assert_eq!(MathStyle::Text.cramped(), MathStyle::TextCramped);
        assert_eq!(MathStyle::TextCramped.cramped(), MathStyle::TextCramped);
    }

    #[test]
    fn size_categories() {
        assert_eq!(MathStyle::Display.size_category(), SizeCategory::Text);
        assert_eq!(MathStyle::TextCramped.size_category(), SizeCategory::Text);
        assert_eq!(MathStyle::Script.size_category(), SizeCategory::Script);
        assert_eq!(
            MathStyle::ScriptScriptCramped.size_category(),
            SizeCategory::ScriptScript
        );
    }

    #[test]
    fn superscript_style_transitions() {
        assert_eq!(MathStyle::Display.superscript_style(), MathStyle::Script);
        assert_eq!(MathStyle::Text.superscript_style(), MathStyle::Script);
        assert_eq!(
            MathStyle::Script.superscript_style(),
            MathStyle::ScriptScript
        );
        assert_eq!(
            MathStyle::ScriptScript.superscript_style(),
            MathStyle::ScriptScript
        );
        assert_eq!(
            MathStyle::DisplayCramped.superscript_style(),
            MathStyle::ScriptCramped
        );
    }

    #[test]
    fn subscript_style_is_always_cramped() {
        assert!(MathStyle::Display.subscript_style().is_cramped());
        assert!(MathStyle::Text.subscript_style().is_cramped());
        assert_eq!(
            MathStyle::Display.subscript_style(),
            MathStyle::ScriptCramped
        );
    }

    #[test]
    fn numerator_style_steps_down_preserving_crampedness() {
        assert_eq!(MathStyle::Display.numerator_style(), MathStyle::Text);
        assert_eq!(MathStyle::Text.numerator_style(), MathStyle::Script);
        assert_eq!(MathStyle::Script.numerator_style(), MathStyle::ScriptScript);
        assert_eq!(
            MathStyle::ScriptScript.numerator_style(),
            MathStyle::ScriptScript
        );
        assert_eq!(
            MathStyle::TextCramped.numerator_style(),
            MathStyle::ScriptCramped
        );
    }

    #[test]
    fn denominator_style_steps_down_and_is_always_cramped() {
        assert_eq!(
            MathStyle::Display.denominator_style(),
            MathStyle::TextCramped
        );
        assert!(MathStyle::Display.denominator_style().is_cramped());
    }

    #[test]
    fn radicand_style_is_cramped_same_size() {
        assert_eq!(
            MathStyle::Display.radicand_style(),
            MathStyle::DisplayCramped
        );
        assert_eq!(
            MathStyle::Display.radicand_style().size_category(),
            SizeCategory::Text
        );
    }
}
