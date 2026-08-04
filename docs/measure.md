# Scaled Line Measurement (`out-glyph-measure`)

`tools/out-glyph-measure` reports the real-world length of geometry drawn on
a PDF page, given that page's drawing scale. It's built for scanned/drafted
technical drawings (architectural plans, engineering diagrams) where a line
on the page represents a real-world distance at some known ratio.

It reuses `tpt-glyph-pdf-measure`'s content-stream geometry walker (Phase 14)
to find every filled/stroked path on a page and measure its length in PDF
units (points, 1/72 inch — exact regardless of the page's actual print
size), then converts that to a real-world length under the page's scale.

## Usage

```sh
# List every painted path on page 1 with its length in millimeters.
out-glyph-measure drawing.pdf --page 1 --scale "1:100"

# Measure just one path (index as listed above), in feet.
out-glyph-measure drawing.pdf --page 1 --path-index 2 --scale "1/4in=1ft" --unit ft

# Different scales on different pages of the same document.
out-glyph-measure drawing.pdf --page 3 --scale "1:100" --page-scale "3=1:50"

# Load a scale table from a config file instead.
out-glyph-measure drawing.pdf --page 1 --scale-file scales.json
```

Output is one line per measured path:

```
path 0: Fill, 280.000 pdf units -> 9877.778mm
path 1: Stroke, 254.558 pdf units -> 8980.256mm
```

`Fill`/`Stroke`/`FillEvenOdd`/`FillAndStroke` is the paint operator that
produced the path (see `tpt-glyph-pdf-measure::PaintKind`); the length is
its total polyline length after Bézier flattening.

### CLI flags

| Flag | Meaning |
|------|---------|
| `<input>` | Path to the PDF (positional, required). |
| `--page <N>` | 1-based page number to measure (default `1`). |
| `--path-index <I>` | Measure only path `I` (0-based). Omit to list every path on the page. |
| `--scale <SPEC>` | Default scale applied to any page without its own override. |
| `--page-scale <PAGE>=<SPEC>` | Per-page scale override; repeatable. |
| `--scale-file <PATH>` | Load a scale table from JSON (see below). Combined with `--scale`/`--page-scale`, which take precedence. |
| `--unit <mm\|cm\|m\|in\|ft>` | Output unit for real-world lengths (default `mm`). |

If no scale applies to a page at all (`--scale` and `--scale-file` both
absent, and no matching `--page-scale`), the tool falls back to 1:1 — the
reported "real-world" length is then just the PDF length in millimeters.

## Scale-spec format

A scale spec (`--scale`, `--page-scale`'s right-hand side, or a JSON entry)
is either:

- **A ratio**, `A:B` — `B` real-world units per `A` drawn units, in any
  consistent unit (the unit cancels out of a pure ratio). Common examples:
  `1:50`, `1:100`, `1:500`.
- **An architectural/engineering equivalence**, `<drawn>=<real>`, where each
  side is a value (a decimal or a simple `a/b` fraction) followed by a unit
  suffix (`mm`, `cm`, `m`, `in`, `ft`):
  - `1/4in=1ft` — the standard 1:48 architectural scale (a quarter inch on
    paper represents one foot in reality).
  - `3/32in=1ft` — 1:128.
  - `5mm=1m` — a 1:200 metric site-plan scale.

Both forms reduce to the same thing internally: a dimensionless real/drawn
factor. `1/4in=1ft` and `1:48` are exactly equivalent scale specs.

## Config file format

`--scale-file` loads a JSON object with an optional `default` and an
optional `pages` map (keys are page numbers as strings):

```json
{
  "default": "1:100",
  "pages": {
    "1": "1/4in=1ft",
    "3": "1:50"
  }
}
```

Page 1 uses the architectural override, page 3 uses 1:50, and every other
page falls back to the 1:100 default.

## Worked example

A 200×200pt PDF page draws a rectangle whose perimeter is exactly 280 PDF
units (a `10 10 80 60 re f` — a 80×60 rectangle) at a 1:100 scale:

```
280 pdf units × (25.4 mm / 72 pdf units per inch) × 100 = 9877.78 mm ≈ 9.88 m
```

That matches the sample output above — a fixture rectangle meant to
represent an 80×60-*point* shape drawn at 1:100 corresponds to a roughly
9.9-meter real-world perimeter (280 points ≈ 3.89 inches on paper, and
3.89 in × 100 ≈ 389 in ≈ 9.88 m).

## Limitations

- **PDF input only.** PostScript isn't supported — `tpt-glyph-pdf-measure`'s
  geometry walker operates on the parsed PDF content-stream IR
  (`tpt-glyph-pdf-ir`), and there's no PostScript equivalent yet.
- **Vector path geometry only.** Text and raster images aren't measured;
  only filled/stroked path geometry (lines, curves, rectangles) counts
  toward a path's length.
- **No path selection UI.** There's no way to click a specific line in a
  viewer — you list all paths on a page, find the index of the one you
  want, and re-run with `--path-index`.

