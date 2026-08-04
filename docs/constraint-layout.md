# Investigation: Constraint-Based Layout for `tpt-glyph-typeset`

Status: **exploratory / design note** (v2.0 cross-cutting goal). No code change
required yet; this records the findings and a recommended path forward.

## Motivation

`tpt-glyph-typeset` currently uses a greedy line-breaking + justification +
pagination model (see `crates/tpt-glyph-typeset/src/layout.rs`). Greedy
first-fit works well for body text but cannot model layout problems where the
optimal break depends on later content — e.g.:

- Balanced multi-column or flowing-around-figure layouts.
- "Best" page breaks that look ahead at the next paragraph's first line.
- Tables / equations whose widths are coupled to available measure.
- Minimum-raggedness over a whole paragraph (Knuth–Plass is a dynamic program,
  not a constraint solver, but couples naturally with one for page-level goals).

A constraint solver lets us state these as *relationships* and have the engine
find a globally-consistent assignment, instead of baking order-dependent
heuristics into the breaker.

## Inspiration: `tpt-telos` QF_LRA

`tpt-telos` (a separate TPT Solutions planning project) solves scheduling and
allocation problems with a **QF_LRA** solver — Quantifier-Free Linear Real
Arithmetic. That fragment covers exactly the constraints a text engine needs:

- Linear inequalities: `x_left + width <= page_right`, `baseline_i +
  leading <= baseline_{i+1}`.
- Variable bounds: `0 <= indent <= measure - width`.
- Equality chains: a paragraph's `block_width = measure - margin_l - margin_r`.

QF_LRA is decidable in polynomial time and admits an incremental Simplex /
tableau implementation, so it is cheap enough to run per-page during layout.

## Proposed Model (sketch)

1. **Variables.** One real variable per measured box edge / position:
   `x0_i, x1_i, y0_i, y1_i` (or `left_i, width_i, top_i, height_i`).
2. **Fixed constraints.** Glyph advance widths and font metrics are constants
   (from `tpt-glyph-font`); boxes are sized from content.
3. **Relational constraints.** Flow/glue, indentation, column and page
   boundaries become linear constraints over the variables.
4. **Objective.** Minimize total raggedness / overflow penalty (a linear or
   piece-wise-linear cost) so the solver picks the best break set, not just any
   feasible one.
5. **Fallback.** When the constraint set is unsatisfiable (content exceeds the
   page), degrade to the existing greedy breaker and emit an overflow warning,
   preserving current behavior.

## Open Questions / Risks

- **Incremental vs per-page solving.** Per-paragraph QF_LRA is cheap; per-document
  coupling (look-ahead page breaks) may need an incremental solver to stay fast.
- **Integration point.** Whether to replace the breaker in `layout.rs` or add a
  `solve` module that `layout` calls for "hard" pages (multi-column, figures).
- **Dependencies.** QF_LRA is small enough to implement in-repo (no heavy
  solver dependency), keeping `tpt-glyph-typeset` `no_std`(+`alloc`)-friendly.
- **Determinism.** A solver must yield identical output across platforms for the
  visual-diff harness to stay meaningful.

## Recommendation

Adopt constraint-based layout *selectively*: keep greedy breaking as the default
fast path, and gate an optional QF_LRA-based solver behind a `constraint-layout`
feature for multi-column / figure-flow / global-raggedness pages. Prototyping
should start from `tpt-telos`'s tableau code as a reference, ported to
`tpt-glyph-typeset`'s coordinate model.

No action is required to ship v2.0.0; this remains a post-2.0 enhancement.
