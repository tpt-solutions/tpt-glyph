# TPT Glyph

> The Ghostscript Successor — a secure, sandboxed, multi-threaded PDF/PostScript rendering engine.

TPT Glyph is a Rust rendering engine for PDF and PostScript documents. It exists to
replace Ghostscript's 30-year-old C codebase, whose reliance on **global mutable
state** makes concurrent rendering unsafe and the code notoriously hard to
maintain. TPT Glyph eliminates that hazard at the architectural level.

## Why it exists

Ghostscript's global state variables are corrupted when two threads render at the
same time. TPT Glyph instead passes the **graphics state as an immutable context
struct** down the rendering tree. Because the state is immutable and contains no
shared mutable references, the engine can render many pages of a document
simultaneously across CPU cores — safely, by construction.

## Architecture overview

```
            ┌─────────────────────────────────────────────┐
            │        PostScript / PDF input (untrusted)    │
            └───────────────────┬─────────────────────────┘
                                 │
                     ┌───────────▼───────────┐
                     │   Parser / Tokenizer   │  (Phase 4, 5)
                     └───────────┬───────────┘
                                 │ operator stream
                     ┌───────────▼───────────┐
                     │  Operator Dispatch     │  driven by the
                     │  (knowledge graph)     │  Knowledge Graph
                     └───────────┬───────────┘
                                 │
         ┌───────────────────────▼───────────────────────┐
         │  Immutable GraphicsState context (no globals)  │
         └───────────────────────┬───────────────────────┘
                                 │ draw commands
             ┌───────────────────▼───────────────────┐
             │  Backend-agnostic Canvas (RGBA buffer)  │
             └───────┬───────────────────────┬────────┘
                     │                       │
             ┌───────▼───────┐       ┌───────▼───────┐
             │  wgpu (GPU)   │       │ raqote (CPU)  │
             └───────────────┘       └───────────────┘

    Per-page rendering is dispatched across a rayon thread pool.
```

### Crate layout

| Crate | Purpose |
|-------|---------|
| `crates/tpt-glyph-core` | Engine library: immutable `GraphicsState`, geometry, canvas, document model, reference + raqote rasterizers, backend selection. |
| `crates/tpt-glyph-cli` | `tpt-glyph` binary — render/convert commands. |
| `crates/tpt-glyph-kg`  | Rendering Pipeline Knowledge Graph (operators → graphics state → pixel buffer). |
| `crates/tpt-glyph-diag`| AI-assisted diagnostic tool consuming the knowledge graph. |
| `crates/tpt-glyph-ps`  | PostScript interpreter (tokenizer, parser, stacks, KG-driven dispatch). |
| `crates/tpt-glyph-pdf` | PDF parsing & rendering (page tree, content streams, fonts/XObjects). |
| `tools/tpt-glyph-diff` | Pixel-diff harness (TPT Glyph vs Ghostscript reference). |
| `tools/tpt-glyph-fixtures` | Generator for synthetic multi-page PDF stress fixtures. |

### Knowledge Graph

The AI strategy behind TPT Glyph ingests PostScript operator definitions and
extracts a **Rendering Pipeline Knowledge Graph**. It maps how vector commands
(`moveto`, `lineto`, `curveto`) translate into pixel-buffer effects, explicitly
isolating the **Graphics State** (color, line width, matrices) as a distinct,
isolated sub-graph. The interpreter's dispatch table is driven by this graph.

## Usage

Render a document to PNG with the CLI:

```sh
# Render all pages of a PDF at 150 DPI, concurrently, auto backend.
tpt-glyph render input.pdf ./out --dpi 150 --parallel

# Render a specific page range with the raqote CPU backend.
tpt-glyph render doc.ps ./out --pages 2-5 --backend cpu-raqote
```

Use the library directly:

```rust,no_run
use tpt_glyph_core::backend::SelectedBackend;
use tpt_glyph_core::geometry::{CubicBezier, Path, Point, Subpath};
use tpt_glyph_core::graphics_state::{GraphicsState, RgbColor};
use tpt_glyph_core::render::RenderTree;

let mut tree = RenderTree::new(64, 64);
let state = GraphicsState::new().with_fill_color(RgbColor::new(0.9, 0.2, 0.1));
tree.fill(&state, Path {
    subpaths: vec![Subpath {
        start: Point::new(8.0, 8.0),
        segments: vec![
            CubicBezier { start: Point::new(8.0, 8.0),  control1: Point::new(32.0, 8.0),  control2: Point::new(32.0, 8.0),  end: Point::new(56.0, 8.0) },
            CubicBezier { start: Point::new(56.0, 8.0), control1: Point::new(56.0, 32.0), control2: Point::new(56.0, 32.0), end: Point::new(56.0, 56.0) },
            CubicBezier { start: Point::new(56.0, 56.0),control1: Point::new(32.0, 56.0), control2: Point::new(32.0, 56.0), end: Point::new(8.0, 56.0) },
            CubicBezier { start: Point::new(8.0, 56.0), control1: Point::new(8.0, 32.0), control2: Point::new(8.0, 32.0), end: Point::new(8.0, 8.0) },
        ],
        closed: true,
    }],
});
let canvas = SelectedBackend::auto(None).rasterize(&tree).unwrap();
assert_eq!(canvas.width, 64);
```

### Rendering untrusted input

When rendering documents from untrusted sources, enable the strict resource
limits so a malicious program cannot exhaust memory, stack depth, or CPU:

```rust,no_run
use tpt_glyph_ps::{Interpreter, ResourceLimits};

let mut interp = Interpreter::with_limits(612, 792, ResourceLimits::strict());
let _ = interp.run_source("(untrusted).ps source");
```

## Status

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Project setup & licensing | ✅ complete |
| 1 | Ghostscript diff harness | ✅ complete |
| 2 | Core architecture | ✅ complete |
| 3 | Knowledge graph subsystem | ✅ complete |
| 4 | PostScript interpreter | ✅ complete |
| 5 | PDF parsing & rendering | ✅ complete |
| 6 | Rasterization backends | ✅ CPU reference + raqote; GPU (wgpu) pending |
| 7 | Multi-threaded rendering | ✅ concurrent PDF render + stress/race tests |
| 8 | CLI & library polish | ✅ public API, `--help`, rustdoc |
| 9 | AI diagnostic tool | ✅ complete |
| 10 | Sandboxing & security | ✅ resource limits, fuzzing, SECURITY.md (OS sandbox pending) |
| 11 | Release prep | 🟡 CHANGELOG + v1.0.0 tagging |

See [`todo.md`](./todo.md) for the full checklist.

## Building

```sh
cargo build --workspace
cargo test  --workspace
```

## Security

TPT Glyph is designed to process untrusted documents. See
[`SECURITY.md`](./SECURITY.md) for the threat model, vulnerability reporting
process, and the defenses already in place (immutable state, resource limits,
fuzzing).

## Licensing

Dual-licensed under either of:

- MIT license ([`LICENSE-MIT`](./LICENSE-MIT))
- Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE))

at your option.

Copyright © TPT Solutions.
