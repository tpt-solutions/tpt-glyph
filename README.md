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
| `crates/glyph-core` | Engine library: immutable `GraphicsState`, geometry, canvas, document model, parallel renderer. |
| `crates/glyph-cli` | `glyph` binary — render/convert commands. |
| `crates/glyph-kg`  | Rendering Pipeline Knowledge Graph (operators → graphics state → pixel buffer). |
| `crates/glyph-diag`| AI-assisted diagnostic tool consuming the knowledge graph. |

### Knowledge Graph

The AI strategy behind TPT Glyph ingests PostScript operator definitions and
extracts a **Rendering Pipeline Knowledge Graph**. It maps how vector commands
(`moveto`, `lineto`, `curveto`) translate into pixel-buffer effects, explicitly
isolating the **Graphics State** (color, line width, matrices) as a distinct,
isolated sub-graph. The interpreter's dispatch table is driven by this graph.

## Status

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Project setup & licensing | ✅ in progress |
| 1 | Ghostscript diff harness | ⏳ planned |
| 2 | Core architecture | 🟡 scaffolding |
| 3 | Knowledge graph subsystem | 🟡 scaffolding |
| 4 | PostScript interpreter | ⏳ planned |
| 5 | PDF parsing & rendering | ⏳ planned |
| 6 | Rasterization backends | ⏳ planned |
| 7 | Multi-threaded rendering | ⏳ planned |
| 8 | CLI & library polish | ⏳ planned |
| 9 | AI diagnostic tool | ⏳ planned |
| 10 | Sandboxing & security | ⏳ planned |
| 11 | Release prep | ⏳ planned |

See [`todo.md`](./todo.md) for the full checklist.

## Building

```sh
cargo build --workspace
cargo test  --workspace
```

## Licensing

Dual-licensed under either of:

- MIT license ([`LICENSE-MIT`](./LICENSE-MIT))
- Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE))

at your option.

Copyright © TPT Solutions.
