# Contributing to TPT Glyph

Thank you for your interest in contributing to TPT Glyph — the secure, sandboxed,
multi-threaded PDF/PostScript rendering engine.

## Licensing

By contributing, you agree that your contributions are dual-licensed under
**MIT OR Apache-2.0**, at your option, matching the project's license.

Every source file must carry the SPDX header at the very top:

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — <crate> / <module>
//
// <one-line purpose>
```

Run `cargo fmt --all` and ensure `cargo clippy --workspace --all-targets -- -D warnings`
passes before opening a pull request.

## Repository layout

| Path | Purpose |
|------|---------|
| `crates/tpt-glyph-core` | Engine: immutable `GraphicsState`, geometry, canvas, rasterizer, backends. |
| `crates/tpt-glyph-cli` | The `glyph` binary (render/convert commands). |
| `crates/tpt-glyph-kg` | Knowledge graph (operators → state → pixel buffer). |
| `crates/out-glyph-diag` | AI-assisted diagnostic tool. |
| `crates/tpt-glyph-ps` | PostScript interpreter. |
| `crates/tpt-glyph-pdf` | PDF parsing/rendering. |
| `tools/out-glyph-diff` | Pixel-diff harness (TPT Glyph vs Ghostscript). |
| `tools/out-glyph-fixtures` | Generator for synthetic multi-page PDF stress fixtures. |
| `fuzz/` | `cargo-fuzz` targets for the parser/interpreter (Phase 10). |
| `fixtures/` | Visual-diff corpus and generated renders. |
| `docs/` | Architecture and testing documentation. |

## Architecture principles

1. **No global mutable state.** The graphics state is an immutable `GraphicsState`
   context passed down the rendering tree. Operators derive a *new* state rather
   than mutating a global. This is what makes concurrent per-page rendering safe.
2. **Backend-agnostic pipeline.** Operators emit `DrawCommand`s into a
   `RenderTree`. A `Rasterizer` (currently the reference software backend in
   `tpt-glyph-core::raster`, with the optional `raqote` CPU backend) turns the tree
   into pixels. GPU/CPU backends implement the same trait.
3. **Knowledge-graph-driven operators.** The interpreter dispatch table is
   derived from the `tpt-glyph-kg` catalog; the `out-glyph-diag` tool verifies the two
   stay in sync.
4. **Fail closed on untrusted input.** The PostScript interpreter enforces
   `ResourceLimits` (operand-stack size, exec-stack depth, draw-command count,
   instruction budget). When adding parser/interpreter paths, preserve these
   bounds and add a regression test that trips them.

## Development workflow

```sh
cargo build --workspace
cargo test  --workspace
cargo fmt   --all
cargo clippy --workspace --all-targets -- -D warnings
```

### Running the visual-diff harness

See [docs/testing.md](./docs/testing.md). In short: add a fixture to
`fixtures/ps/` or `fixtures/pdf/`, then run `pwsh tools/run-diff.ps1`.

### Knowledge graph & diagnostics

```sh
cargo run -p out-glyph-diag -- coverage     # operator coverage report
cargo run -p out-glyph-diag -- validate     # dispatch table vs graph consistency
cargo run -p out-glyph-diag -- build --export fixtures/kg.json
```

### Fuzzing untrusted input (Phase 10)

Fuzz targets live in `fuzz/` and require a nightly toolchain with `cargo-fuzz`:

```sh
cargo +nightly fuzz run ps_interpreter   # PostScript interpreter (strict limits)
cargo +nightly fuzz run pdf_content      # PDF content-stream / decoder path
```

New crash artifacts go under `fuzz/artifacts/` (git-ignored). Triage: confirm the
crash is not already covered by a `ResourceLimits` guard, reduce the input with
`cargo +nightly fuzz tmin`, then add a regression test in the relevant crate's
`tests/`.

## Commit / PR guidelines

- Keep PRs focused; describe the *why* as well as the *what*.
- Add or update tests for new operators, rasterization paths, or backends.
- CI must stay green (build, lint, visual-diff).

