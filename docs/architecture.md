# Architecture Decision Record: Immutable Graphics State

Status: accepted

## Context

Ghostscript stores its drawing context (current color, line width, transformation
matrix, clip path, font, etc.) in a set of **global mutable variables** in C.
When two threads render documents concurrently, both threads read and write the
same globals, producing corrupted output, crashes, or security vulnerabilities.
The codebase is also extremely difficult to reason about because any operator can
mutate any piece of state at any time.

## Decision

In TPT Glyph, the graphics state is represented by the immutable struct
`GraphicsState` (`crates/glyph-core/src/graphics_state.rs`). Operators **do not
mutate** the state; instead, an operator receives the current `GraphicsState` and
returns a *new* state (e.g. `GraphicsState::with_stroke_color`,
`GraphicsState::concat_transform`). The new state is then threaded into subsequent
operators or saved/restored via `gsave`/`grestore` stacks.

The rendering pipeline operates on this context and emits draw commands into a
backend-agnostic `Canvas` (`crates/glyph-core/src/canvas.rs`), which owns a
row-major RGBA8 buffer.

## Consequences

### Benefits

- **Thread safety by construction.** `GraphicsState` contains only `Copy`,
  `Clone` primitive/value types — no shared mutable references. It can be cloned
  and moved into another thread without locks. This is what enables safe
  per-page parallel rendering via a `rayon` thread pool
  (`render_document_parallel` in `document.rs`).
- **Auditability.** Because every state transition is an explicit, value-based
  transformation, the full effect of any operator is local and inspectable.
- **Testability.** Operator semantics can be unit-tested purely as
  `GraphicsState -> GraphicsState` (and `Path`) functions with no global setup.

### Trade-offs

- Operators must explicitly thread the state through the call stack, which is more
  verbose than reading a global. This is accepted as the price of safety.
- `gsave`/`grestore` require maintaining an explicit stack of saved states rather
  than saving/restoring C globals.

## Relationship to the Knowledge Graph

The Rendering Pipeline Knowledge Graph (`crates/glyph-kg`) explicitly models the
"Graphics State" as an isolated sub-graph of nodes (color, line width, CTM, clip
path). The interpreter's operator dispatch table is generated/validated against
this graph, guaranteeing that every operator's effect on the isolated graphics
state is documented and covered.
