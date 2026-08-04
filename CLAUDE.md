# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

TPT Glyph is a Rust PDF/PostScript rendering engine — "the Ghostscript successor." Its
central design goal is to eliminate Ghostscript's global-mutable-state hazard: the
graphics state is an **immutable value** threaded through the rendering pipeline
instead of a set of global C variables, which makes concurrent per-page rendering
safe by construction. It's also built to process **untrusted** input (PDF/PostScript
from unknown sources), so resource limits and fuzzing are first-class concerns, not
afterthoughts.

Full narrative docs already exist and should be treated as authoritative for anything
not covered here: [README.md](README.md) (usage, crate table, status),
[docs/architecture.md](docs/architecture.md) (the immutable-`GraphicsState` ADR),
[docs/testing.md](docs/testing.md) (visual-diff harness), [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)
(architecture principles + SPDX header convention), and [SECURITY.md](SECURITY.md)
(threat model).

## Commands

```sh
# Build / test everything
cargo build --workspace
cargo test  --workspace

# Run a single test
cargo test -p tpt-glyph-pdf renders_all_pages_sequentially   # by name, any crate
cargo test -p tpt-glyph-pdf --test concurrency               # one integration test file

# Format + lint (both are CI gates; clippy denies warnings)
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# MSRV check (toolchain 1.85, pinned in Cargo.toml / clippy.toml)
cargo check --workspace --all-targets

# Benchmarks (criterion, tpt-glyph-core only)
cargo bench -p tpt-glyph-core

# Run the CLI (binary name is `tpt-glyph`, not `glyph`)
cargo run -p tpt-glyph-cli -- render input.pdf ./out --dpi 150 --parallel --backend cpuraqote

# Knowledge-graph diagnostics
cargo run -p out-glyph-diag -- coverage     # operator implementation coverage
cargo run -p out-glyph-diag -- validate     # dispatch table vs. KG consistency (fails non-zero on mismatch)
cargo run -p out-glyph-diag -- build --export fixtures/kg.json

# Fuzzing (requires nightly + cargo-fuzz; targets in fuzz/fuzz_targets/)
cargo +nightly fuzz run ps_interpreter
cargo +nightly fuzz run pdf_content

# Visual-diff harness vs. Ghostscript reference renders (Windows helper)
pwsh tools/run-diff.ps1                    # generate refs (needs Docker) + candidates + diff
pwsh tools/run-diff.ps1 -SkipReferences    # reuse existing reference PNGs
```

Note: `crates/tpt-glyph-core` gates its accelerated backend behind Cargo features
(`raqote-backend`, `wgpu-backend`); `tpt-glyph-cli` always enables `raqote-backend`.
When testing `tpt-glyph-core` in isolation, pass `--features raqote-backend` if you
need that path exercised.

## Architecture

### Data flow

Both untrusted-input paths converge on the same backend-agnostic pipeline:

```
PostScript (.ps)                          PDF (.pdf)
  tpt-glyph-ps::lexer/parser                 tpt-glyph-pdf (via the `pdf` crate)
        │                                          │
        │ operator stream                          │ content-stream ops (pdf::content::Op)
        ▼                                          ▼
  tpt-glyph-ps::interpreter                  tpt-glyph-pdf::content
  (dispatch table built from                (maps ops directly onto the
   the tpt-glyph-kg catalog)                 same core primitives)
        └──────────────────┬───────────────────────┘
                            ▼
          immutable GraphicsState (tpt-glyph-core::graphics_state)
          threaded value-to-value through operators — never mutated in place
                            │ draw commands
                            ▼
             RenderTree (tpt-glyph-core::render) — backend-agnostic
                            │
                            ▼
        Rasterizer trait: SoftwareRasterizer (reference, always on)
                        or raqote CpuRaqote backend (feature-gated)
                        or wgpu Gpu backend (stubbed, falls back to CPU)
                            │
                            ▼
              Canvas (tpt-glyph-core::canvas) — RGBA8 buffer, save_png()
```

`tpt-glyph-cli` (`crates/tpt-glyph-cli/src/main.rs`) is the thing that wires all of
this together for the `render` subcommand: it picks PDF vs. PS by file extension,
resolves the backend via `SelectedBackend::auto`, and — when `--parallel` is passed —
renders pages concurrently across a `rayon` thread pool. This concurrency is only
safe because `GraphicsState` and the per-page render path hold no shared mutable
state; `crates/tpt-glyph-pdf/tests/concurrency.rs` is the regression test proving
pages don't leak state into each other under parallel rendering.

### Knowledge graph (`crates/tpt-glyph-kg`)

The interpreter's operator dispatch table isn't hand-maintained in isolation — it's
generated from a declarative catalog (`catalog.rs`) of `OperatorDef`s, each of which
declares which graphics-state attributes it touches and which pixel-buffer effects it
produces. `ingest.rs` turns the catalog into a `KnowledgeGraph` (nodes + edges);
`validate.rs` derives the actual dispatch table from the same catalog. The graph
deliberately isolates graphics-state attributes (`stroke_color`, `ctm`, `clip_path`,
...) as their own node kind — this is the same isolation principle as the immutable
`GraphicsState` struct, just modeled explicitly.

`out-glyph-diag` (`crates/out-glyph-diag/src/main.rs`) is the consumer: `validate`
checks the KG and the live dispatch table haven't drifted apart, `coverage` reports
which cataloged operators are actually implemented, and `diff` cross-references a
visual-diff `diff-report.json` against unimplemented operators to suggest likely
causes for a failing fixture. When adding a new PostScript/PDF operator, update
`catalog.rs` first — the dispatch table and the KG both derive from it, so they
can't drift if that's the only edit.

### Resource limits (`crates/tpt-glyph-ps::limits`)

`ResourceLimits` bounds operand-stack size, exec-stack depth, emitted draw-command
count, and total instruction count for a single interpreter run. Three presets:
`default()`, `strict()` (for fully untrusted input), `unbounded()` (tests/benchmarks
only). Any new parser/interpreter code path that can loop, recurse, or grow a
stack/buffer based on document content must respect these bounds — add a regression
test that trips the limit, per [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).

### Visual-diff harness (`tools/out-glyph-diff`, `tools/out-glyph-fixtures`)

Correctness is validated by pixel-diffing TPT Glyph's output against Ghostscript
reference renders (MSE / peak-error / SSIM against `fixtures/thresholds.json`), not
just unit tests — geometry/color bugs that unit tests wouldn't catch show up here.
Fixtures live in `fixtures/ps/` and `fixtures/pdf/`; dropping a new file into either
directory is picked up automatically by the harness, no code change needed. See
[docs/testing.md](docs/testing.md) for the full loop and the CI wiring
(`visual-diff` job in `.github/workflows/ci.yml`).

### Crate/tool layout

| Path | Role |
|------|------|
| `crates/tpt-glyph-core` | `GraphicsState`, geometry, `Canvas`, `RenderTree`, `Rasterizer`/`Backend` abstraction, reference + raqote rasterizers. |
| `crates/tpt-glyph-cli` | `tpt-glyph` binary — wires PS/PDF input, KG-driven interpreter, and backends together behind `render`/`version`. |
| `crates/tpt-glyph-kg` | Knowledge graph: operator catalog, ingestion, KG↔dispatch-table validation. |
| `crates/out-glyph-diag` | CLI consuming the KG for coverage/validate/diff-report analysis. |
| `crates/tpt-glyph-ps` | PostScript lexer/parser/interpreter, operand/exec stacks, `ResourceLimits`. |
| `crates/tpt-glyph-pdf` | PDF parsing (via the `pdf` crate) and content-stream → core-pipeline mapping. |
| `tools/out-glyph-diff` | Pixel-diff comparator (MSE/peak-error/SSIM) for the visual-diff harness. |
| `tools/out-glyph-fixtures` | Generates synthetic multi-page PDF stress fixtures. |
| `fuzz/` | `cargo-fuzz` targets: `ps_interpreter`, `pdf_content` (nightly toolchain only). |

## Conventions

- Every source file starts with an SPDX header (`// SPDX-License-Identifier: MIT OR
  Apache-2.0`, followed by a `// TPT Glyph — <crate>/<module>` line and a one-line
  purpose comment) — see any existing file, e.g.
  [crates/tpt-glyph-core/src/lib.rs](crates/tpt-glyph-core/src/lib.rs).
- Never introduce mutable global/shared state into the rendering path. Operators
  transform `GraphicsState` by returning a new value (`with_fill_color`,
  `concat_transform`, ...), never by mutating in place — this is the entire point
  of the architecture (see [docs/architecture.md](docs/architecture.md)).

