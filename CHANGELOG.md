# Changelog

All notable changes to TPT Glyph are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Phase 6 — raqote CPU backend.** A new `raqote`-backed rasterizer
  (`tpt-glyph-core::backends::raqote`) implementing the `Rasterizer` trait, used as
  the deterministic antialiased CPU fallback. Gated behind the `raqote-backend`
  feature; auto-selected when compiled in. (`#Backend::CpuRaqote`)
- **Phase 6 — backend parity test.** Reference and raqote backends are asserted
  to render the same document equivalently (edge-only divergence tolerance).
- **Phase 6 — rasterization benchmarks.** `cargo bench -p tpt-glyph-core
  --features raqote-backend` compares the reference and raqote backends.
- **Phase 7 — concurrent rendering correctness.** Stress + race tests render a
  synthetic multi-page PDF across a rayon pool and prove deterministic,
  leak-free per-page output.
- **Phase 8 — library rustdoc** with a runnable usage example; polished
  `tpt-glyph render --help` output.
- **Phase 10 — resource limits.** `tpt_glyph_ps::ResourceLimits` (operand-stack
  size, execution-stack depth, draw-command count, instruction budget) enforce
  fail-closed execution of untrusted PostScript. `ResourceLimits::strict()` is
  recommended for untrusted input.
- **Phase 10 — `SECURITY.md`** documenting the threat model, vulnerability
  reporting process, and existing defenses.
- **Phase 10 — fuzzing scaffold.** `cargo-fuzz` targets (`ps_interpreter`,
  `pdf_content`) exercise the untrusted parser/interpreter paths.

### Changed

- `Backend::select` now prefers the raqote backend when available; auto-selection
  resolves to a CPU backend (GPU path still pending).
- `tpt-glyph` CLI exposes `--backend cpu-raqote` in addition to `auto`/`cpu`/`gpu`.

### Tooling

- `tools/out-glyph-fixtures` generates the multi-page PDF stress corpus
  (`fixtures/pdf/multipage-4.pdf`).

## [0.1.0] - 2026-07-18

- Initial scaffold: workspace, dual MIT/Apache-2.0 licensing, CI skeleton.
- Phase 1 Ghostscript visual-diff harness and fixture corpus.
- Phase 2 core architecture (immutable `GraphicsState`, geometry, canvas,
  `Rayon` concurrency, reference software rasterizer).
- Phase 3 knowledge-graph subsystem with coverage/validation tooling.
- Phase 4 PostScript interpreter (26 catalog operators) with KG-driven dispatch.
- Phase 5 PDF parsing & rendering (page tree, content streams, fonts/XObjects).
- Phase 9 `out-glyph-diag` AI-assisted diagnostic tool.

