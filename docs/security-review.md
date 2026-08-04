# Security Review — v2.0 Pre-Release Pass

Scope: a focused source review of the v2.0 workspace against the Phase 10 threat
model (untrusted PDF/PostScript input). This is a code-review pass, not a formal
audit or a fuzzing campaign (see `fuzz/` for continuous coverage).

## Reviewed

- `tpt-glyph-ps` — lexer, parser, operand/exec/graphics-state stacks,
  interpreter resource-limit enforcement (`ResourceLimits`).
- `tpt-glyph-core` — geometry, graphics state, reference rasterizer (Bézier
  flattening recursion), and the `wgpu` GPU backend's one `unsafe` block.
- `tpt-glyph-pdf-parser` / `tpt-glyph-pdf-ir` — entry-point I/O and IR
  population.
- `tpt-glyph-font`, `tpt-glyph-math`, `tpt-glyph-pdf-writer`,
  `tpt-glyph-pdf-measure`, `tpt-glyph-pdf-editor`, `tpt-glyph-typeset` —
  public-API I/O surface.

## Findings & Actions

### F1 (fixed) — Unbounded parser nesting → stack-overflow DoS
`parse_sequence` in `crates/tpt-glyph-ps/src/parser.rs` recursed once per
nested procedure/array with **no depth cap**. A hostile document such as
`[[[[ … ]]]]` with enough nesting would overflow the Rust stack during
parsing — *before* the interpreter's own `max_exec_depth` limit could apply,
because parsing runs first and is purely recursive.

Fix: added `MAX_PARSE_DEPTH` (1000) and a `depth` counter threaded through
`parse_sequence`; nesting beyond the limit now fails closed with
`PsError::Parse`. Added `deeply_nested_array_is_rejected` regression test.

### F2 (ok) — `wgpu` backend `unsafe`
The only `unsafe` in the workspace (`crates/tpt-glyph-core/src/backends/wgpu.rs`,
`as_bytes`) transmutes `Vertex`/`u32` slices to bytes. It is sound: the target
`T` is `#[repr(C)]` with only plain numeric fields and no padding, so every byte
is initialized and any bit pattern is valid. No action needed; the safety
comment documents the invariant.

### F3 (ok) — Bézier flattening & geometry recursion are bounded
`raster.rs` caps Bézier subdivision via `MAX_RECURSION_DEPTH`, and
`tpt-glyph-pdf-measure/src/geometry.rs` caps its walk recursion explicitly.
Both resist adversarial geometry without unbounded stack growth.

### F4 (ok) — Interpreter fail-closed limits
`ResourceLimits` (operand stack, exec depth, draw-command/output count,
instruction budget) is enforced at every step via `check_*`. `strict()` is the
recommended profile for untrusted input. The stacks use `Result`-returning
`pop` (no `unwrap` on underflow). Verified by the existing limit regression
tests.

## Residual / Recommendations (not blocking)

- **Single-string size.** The PS lexer (`read_string`) accumulates one literal
  string with no length cap. Iterative (no stack risk) but a multi-GB literal
  would allocate. Add a max-string-length guard in a future hardening pass if
  inputs are fully untrusted.
- **PDF decompression through the `pdf` crate.** Stream/object-stream decoding
  (Flate) is performed by the third-party `pdf` crate; a decompression bomb in
  a malformed PDF is outside our direct control. There is no in-engine
  instruction/output budget equivalent to the PS `ResourceLimits` on the PDF
  render path yet — rely on the deployer sandbox (per `SECURITY.md`) and
  consider a decoded-stream-size cap as future work.
- **OS-level sandboxing** remains a deployer concern (documented in
  `SECURITY.md`); it is intentionally not an in-engine capability.

## Verdict

No blocking issues found. F1 was a genuine pre-release defect and is fixed and
tested. The workspace is suitable for a v1.0.0 / v2.0.0 release subject to the
residual hardening items above and the external steps (crates.io publish, tag)
tracked in `todo.md`.
