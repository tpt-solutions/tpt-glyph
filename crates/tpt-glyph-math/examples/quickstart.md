# Quickstart

Build a [`MathExpr`] AST and inspect its structure. With the `std` feature the
laid-out tree can be emitted as `tpt-glyph-core` draw commands via
[`emit::typeset`](tpt_glyph_math::emit::typeset); the AST and layout algorithm
are available with default (`no_std` + `alloc`) features, depending only on
`tpt-glyph-font` for glyph metrics.

```rust
use tpt_glyph_math::prelude::*;

// x / y^2 — the worked example from the project spec.
let expr = MathExpr::Fraction {
    numerator: Box::new(MathExpr::Identifier("x".into())),
    denominator: Box::new(MathExpr::Superscript {
        base: Box::new(MathExpr::Identifier("y".into())),
        sup: Box::new(MathExpr::Number("2".into())),
    }),
    bar: FractionBar::Default,
};

match &expr {
    MathExpr::Fraction { numerator, denominator, .. } => {
        assert!(matches!(**numerator, MathExpr::Identifier(_)));
        assert!(matches!(**denominator, MathExpr::Superscript { .. }));
    }
    _ => panic!("expected a fraction"),
}
```
