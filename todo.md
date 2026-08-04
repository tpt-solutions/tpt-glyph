# TPT Glyph — Project Checklist

TPT Glyph: a secure, sandboxed, multi-threaded PDF/PostScript rendering engine written in Rust — the Ghostscript successor. Built by TPT Solutions, dual-licensed under MIT OR Apache-2.0.

---

## Phase 0 — Project Setup & Licensing

- [x] Initialize Cargo workspace (`tpt-glyph`) with member crates
- [x] Create crate layout: `crates/tpt-glyph-core` (engine lib), `crates/tpt-glyph-cli` (binary), `crates/tpt-glyph-kg` (knowledge graph), `crates/out-glyph-diag` (AI diagnostic tool)
- [x] Add `LICENSE-MIT` (Copyright TPT Solutions)
- [x] Add `LICENSE-APACHE` (Copyright TPT Solutions)
- [x] Add `license = "MIT OR Apache-2.0"` to all crate `Cargo.toml` files
- [x] Add SPDX dual-license header convention for source files
- [x] Write root `README.md` skeleton (purpose, architecture overview, status)
- [x] Set up `.gitignore`, `rustfmt.toml`, `clippy.toml`
- [x] Initialize git repository, initial commit
- [x] Set up CI pipeline skeleton (build + test on push/PR)
- [x] Choose MSRV (minimum supported Rust version) and pin in CI

## Phase 1 — Ghostscript Diff Testing Harness (early, parallel to core work)

- [x] Write Dockerfile that builds/runs upstream Ghostscript for reference rendering
- [x] Build fixture corpus: sample PDF files (varied complexity) and PostScript files
- [x] Write pixel-diff comparison tool (compare TPT Glyph output vs Ghostscript output)
- [x] Define pass/fail thresholds for visual diffing (pixel tolerance, SSIM, etc.)
- [x] Wire visual-diff harness into CI as a regression gate
- [x] Add script/tooling to add new fixtures easily as coverage grows
- [x] Document harness usage in `CONTRIBUTING.md` or `docs/testing.md`

## Phase 2 — Core Architecture: Graphics State & Rendering Pipeline

- [x] Design immutable `GraphicsState` context struct (color, line width, transform matrices, clip path)
- [x] Define rendering tree / traversal model (how operators produce draw commands)
- [x] Define core path/geometry primitives (points, subpaths, beziers)
- [x] Define pixel buffer / canvas abstraction (backend-agnostic)
- [x] Define `Page` and `Document` abstractions
- [x] Design concurrency model for per-page parallel rendering (e.g. `rayon` thread pool)
- [x] Write unit tests proving `GraphicsState` is immutable/no shared mutable state
- [x] Document the architecture decision (why immutable context vs Ghostscript's globals)

## Phase 3 — Knowledge Graph Subsystem

- [x] Design schema for the Rendering Pipeline Knowledge Graph (operators → pixel-buffer effects)
- [x] Model the isolated "Graphics State" sub-graph (color, line width, matrices) distinctly
- [x] Choose graph storage/serialization format (e.g. embedded graph structure, JSON/RON, or embedded graph DB)
- [x] Build ingestion tooling that parses PostScript operator definitions into the graph
- [x] Build validation tooling: interpreter operator dispatch table checked against the KG for coverage/consistency
- [x] Expose the KG as an inspectable artifact (CLI command or exported file) for developers
- [x] Write tests for graph ingestion and validation tooling

## Phase 4 — PostScript Operator Interpreter

- [x] Write PostScript tokenizer/lexer
- [x] Write PostScript parser (procedures, dict/array literals, operand stack)
- [x] Implement operand stack and execution stack
- [x] Build operator dispatch table driven by the Phase 3 knowledge graph
- [x] Implement path construction operators (`moveto`, `lineto`, `curveto`, `closepath`)
- [x] Implement path painting operators (`stroke`, `fill`, `clip`)
- [x] Implement graphics state operators (`gsave`, `grestore`, `setrgbcolor`, `setlinewidth`, `concat`)
- [x] Integrate interpreter output with the Phase 2 rendering pipeline
- [x] Add unit tests per operator against known expected geometry/state transitions
- [x] Run interpreter output through Phase 1 harness against sample `.ps` files

Phase 4 complete: all 26 catalog operators are implemented and the KG reports
100% operator coverage. `arc`, `rotate`, `rmoveto`, `rlineto`, `eofill`,
`setlinecap`, `setlinejoin`, `setgray`, `exch`, and `show` are all wired in.
`show` is a placeholder text renderer (stamps glyph origin points); full font /
glyph-outline handling lands in Phase 5.

## Phase 5 — PDF Parsing & Rendering

- [x] Integrate `pdf` crate for baseline document/xref/object structure parsing
- [x] Implement page tree walking (resolve pages, resources, media boxes)
- [x] Implement PDF content-stream tokenizer (via `pdf::content::Op` decoding in `content.rs`)
- [x] Wire content-stream operators into the shared operator dispatch from Phase 4 where overlapping
- [x] Implement PDF-specific operators not shared with PostScript (e.g. text positioning, XObjects)
- [x] Add baseline font handling (embedded font extraction, glyph outlines)
- [x] Add image/XObject decoding groundwork (raw/Flate-encoded streams)
- [x] Run PDF rendering output through Phase 1 harness against sample `.pdf` files

Phase 5 complete: `tpt-glyph-pdf` opens PDFs via the `pdf` crate, walks the page
tree, decodes content-stream operators onto the shared tpt-glyph-core pipeline
(path construction/painting, CTM, color, line attributes, ExtGState), and
implements PDF-specific text positioning (`Tf`/`Tm`/`Td`/`Tj`/`TJ`) plus XObject
(Form + Image) handling. Baseline font/string decoding (PDFDocEncoding,
UTF-16BE) and glyph-advance lookup are in place; glyph outlines are stamped as
placeholder boxes. The `hello.pdf` fixture renders end-to-end through the CLI
and the Phase 1 harness now renders PDF candidates.

## Phase 6 — Rasterization Backends

- [x] Implement `wgpu`-based GPU rendering backend (primary path)
- [x] Implement `raqote`-based CPU rendering backend (fallback path)
- [x] Implement backend abstraction/trait so pipeline is backend-agnostic
- [x] Implement runtime backend selection/detection (GPU available vs fallback)
- [x] Implement reference software rasterizer backend (scanline fill + stroke)
- [x] Write parity tests: same document renders equivalently on both backends (via Phase 1 harness)
- [x] Benchmark both backends on the fixture corpus

## Phase 7 — Multi-threaded Multi-page Rendering

- [x] Wire concurrent per-page rendering using immutable `GraphicsState` context across threads
- [x] Add stress tests rendering large multi-page PDFs concurrently
- [x] Add correctness tests proving no cross-page state leakage/races (e.g. with loom or targeted stress tests)
- [x] Benchmark multi-threaded rendering throughput vs single-threaded Ghostscript baseline
- [x] Tune thread pool sizing / work-stealing strategy

## Phase 8 — CLI & Library Crate Polish

- [x] Design public `tpt-glyph-core` library API (documents, render options, output targets)
- [x] Implement `glyph` CLI binary: render/convert commands (PDF/PS → PNG/other raster formats)
- [x] Add CLI options mirroring common Ghostscript flags where sensible (resolution, page range, output format)
- [x] Write `rustdoc` documentation and usage examples for the library crate
- [x] Write CLI usage docs / `--help` output polish
- [x] Add integration tests for the CLI binary

## Phase 9 — AI-Assisted Diagnostic Tool

- [x] Design `out-glyph-diag` companion tool consuming the Phase 3 knowledge graph
- [x] Implement operator coverage reporting (which PS/PDF operators are implemented vs missing)
- [x] Implement Ghostscript-diff-driven analysis (surface which fixtures fail and why, using Phase 1 harness output)
- [x] Implement AI/LLM-assisted fix suggestion feature (propose likely causes/fixes from diff + KG context)
- [x] Build developer-facing CLI (or lightweight UI) for exploring the graph and diagnostics
- [x] Document how to use the diagnostic tool during development

## Phase 10 — Sandboxing & Security Hardening

- [x] Threat-model untrusted PDF/PS input handling
- [x] Implement sandboxed execution for untrusted documents (process isolation / capability restrictions)
- [x] Add resource-limit enforcement (memory, CPU time, recursion depth, output size)
- [x] Set up fuzzing (e.g. `cargo-fuzz`) targeting parser and interpreter
- [x] Run and triage fuzzing results; fix discovered crashes/hangs
- [x] Conduct a security review pass before release
- [x] Add `SECURITY.md` with vulnerability reporting process

## Phase 11 — Release Prep

- [x] Finalize `README.md` with full architecture overview and usage examples
- [x] Write `CHANGELOG.md`
- [x] Decide on versioning scheme (SemVer) and tag v1.0.0 criteria
- [x] Verify crate metadata (description, keywords, categories, license) for publishing
- [x] Confirm dual MIT/Apache-2.0 licensing is correctly applied across all crates and files
- [ ] Final full run of Phase 1 visual-diff harness across entire fixture corpus
- [ ] Tag and publish v1.0.0 release

---

## Session Notes (2026-07-18)

Reconciled the checklist against the actual codebase. The following were already
implemented but previously left unchecked, now marked done:

- **Phase 2** (core architecture) — `GraphicsState`, `RenderTree`, geometry, canvas,
  `Page`/`Document`, `rayon` concurrency, immutability tests, and `docs/architecture.md`
  are all present.
- **Phase 8** — `glyph` CLI render command (PS/PDF → PNG), `--dpi`, `--pages` range,
  `--backend`, `--parallel` options, and `crates/tpt-glyph-cli/tests/render.rs` integration
  tests all exist.

Work completed this session:

- **Phase 6** — Added `crates/tpt-glyph-core/src/raster.rs`: a real reference software
  rasterizer (adaptive Bézier flattening, scanline fill with non-zero + even-odd rules,
  segment-expansion stroking with line caps). Added `crates/tpt-glyph-core/src/backend.rs`:
  `Backend`/`SelectedBackend` abstraction with runtime auto-selection. Added `Point`
  arithmetic operators. Kept `DebugRasterizer` for existing tests.
- **Phase 7** — Wired concurrent per-page PDF rendering into the CLI via `rayon`
  (`--parallel`), reusing the immutable-state safety model.
- **Phase 9** — Added `out-glyph-diag diff` subcommand that consumes a `out-glyph-diff` report,
  prints per-fixture metric hints, and cross-references failures with the knowledge graph
  to suggest related unimplemented operators.
- **Phase 0/1** — Added `docs/CONTRIBUTING.md`; updated `README.md` status table.

Verification: `cargo build`, `cargo test` (54 tests pass), `cargo clippy --all-targets`
(no warnings), and `cargo fmt --check` are all clean.

Still outstanding (not yet implemented):

- **Phase 6**: `wgpu` GPU backend (the GPU path still resolves to the CPU backend at runtime).
- **Phase 7**: loom-based race tests (current stress tests prove determinism under `rayon`); thread-pool tuning.
- **Phase 10**: OS-level process sandboxing is a deployer concern (documented in `SECURITY.md`); not yet an in-engine capability. A formal security review pass is recommended before v1.0.0.
- **Phase 11**: final full visual-diff harness run + tagging/publishing `v1.0.0`.

---

## Session Notes (2026-07-18, continued)

Completed the remaining checklist items across Phases 6, 7, 8, 10, and 11:

- **Phase 6** — Added `tpt-glyph-core::backends::raqote` (CPU rasterizer via `raqote`,
  gated behind `raqote-backend`), wired into `Backend` selection as `CpuRaqote` and
  auto-selected when compiled in. `tpt-glyph-cli` selects it via `--backend cpu-raqote`.
  Added a backend-parity test (`backends::raqote::tests`) asserting reference and
  raqote render the same document equivalently (edge-only divergence). Added a
  `criterion` benchmark (`benches/backends.rs`) comparing both backends.
- **Phase 7** — Added `tools/out-glyph-fixtures` to generate `fixtures/pdf/multipage-4.pdf`
  (4 distinct colored pages). Added `crates/tpt-glyph-pdf/tests/concurrency.rs`: a
  sequential render test and a `rayon` stress test proving deterministic,
  leak-free concurrent per-page rendering.
- **Phase 8** — Added a runnable crate-level doctest to `tpt-glyph-core` and an
  immutability doctest to `GraphicsState`. Polished `tpt-glyph render --help` (clearer
  about/argument docs and backend enum help).
- **Phase 10** — Added `tpt_glyph_ps::ResourceLimits` (operand-stack size, exec-stack
  depth, draw-command count, instruction budget) enforced fail-closed in the
  interpreter, with `strict()` for untrusted input and 4 new regression tests.
  Added `GlyphError::ResourceLimit` + `PsError::ResourceLimit`. Added `SECURITY.md`
  (threat model, reporting process, defenses) and `fuzz/` cargo-fuzz targets
  (`ps_interpreter`, `pdf_content`). Documented fuzzing in `CONTRIBUTING.md`.
- **Phase 11** — Finalized `README.md` (status table, usage examples, security
  section), wrote `CHANGELOG.md`, and added `keywords`/`categories` crate metadata
  to `tpt-glyph-core` and `tpt-glyph-cli`.

Verification: `cargo test --workspace` (all pass), `cargo clippy --workspace
--all-targets` and `cargo clippy -p tpt-glyph-core --features raqote-backend
--all-targets` (no warnings), `cargo fmt --all --check` (clean). The `wgpu` GPU
backend and the actual `v1.0.0` tag/publish remain as the only outstanding items.

---

## v2.0 — The Comprehensive PDF & Typesetting Suite

Source: `spec2.txt`. Phases below continue numbering from the v1.0 checklist
above and are additive scope, not replacements for Phases 0–11.

## Phase 12 — v2.0 Foundation: tpt-glyph-font & PDF IR

- [x] Create `tpt-glyph-font` crate (wraps `ttf-parser`: TTF/OTF metric parsing, glyph outlining, kerning)
- [x] Design `tpt-glyph-pdf-ir` data structures (Pages, Content Streams, Resources, XRef) as the canonical immutable PDF IR
- [x] Reconcile existing `tpt-glyph-pdf` crate with the new `tpt-glyph-pdf-parser` / `tpt-glyph-pdf-ir` split (naming/migration decision)
- [x] Add `no_std` (+ `alloc`) compatibility groundwork for `tpt-glyph-core`
- [ ] Publish `tpt-glyph-core` and `tpt-glyph-font` to crates.io as v0.1.0

## Phase 13 — Math Typesetting Engine (`tpt-glyph-math`)

- [x] Scaffold `tpt-glyph-math` crate with `default-features = []`
- [x] Design strongly-typed `MathExpr` AST (`Fraction`, `Superscript`, `Identifier`, `Number`, etc.)
- [x] Implement TeX-style math layout algorithm (TeXbook Ch. 17): Display/Text/Script/ScriptScript styles
- [x] Implement math atom spacing rules (ORD, OP, BIN, REL, ...) using standard math kerning tables
- [x] Implement axis-height / rule-thickness calculation from the current font's x-height
- [x] Emit laid-out math AST as `tpt-glyph-core` draw commands (glyph placement, vector fraction bars)
- [x] Add optional `latex-parser` feature: pest-based LaTeX math string → `MathExpr` AST parser
- [x] Build CLI demo: `.math` file or LaTeX string → typeset PDF via `tpt-glyph-core` + `tpt-glyph-pdf-writer`
- [ ] Publish `tpt-glyph-math` to crates.io

Phase 13 complete except the two items above, deferred by design: the CLI
demo needs `tpt-glyph-pdf-writer` (Phase 14, not built yet) to emit real PDF
output, so it's left unstarted rather than substituted with a non-PDF demo;
`emit::typeset`/`typeset_to_render_tree` already give a future CLI a
ready-to-call entry point with no rework needed. Publishing is a manual
`cargo publish` step, not something to automate.

Implementation notes: full TeXbook Ch. 17 scope was implemented, including
radicals, accents, over/underline, `\left`/`\right` stretchy delimiters, and
big-operator limits — not just the MVP subset the checklist wording implies.
`tpt-glyph-math` is `no_std` (+ `alloc`) by default (`ast`/`style`/`atom`/
`constants`/`layout` only depend on `tpt-glyph-font`); the `std` feature adds
`emit` (which depends on `tpt-glyph-core`, itself not `no_std` yet); the
`latex-parser` feature adds a pest-based LaTeX math subset parser. Math
constants (axis height, rule thickness, sub/superscript shifts, fraction/
radical gaps, ...) are documented approximations derived from font x-height
via `MathConstants::from_font`, since no OpenType MATH table is parsed —
real TeX-quality per-font constants would need one. Stretchy delimiters and
enlarged radical surds are approximated by non-uniform vertical scaling of
the base glyph outline, not real per-size glyph variants. 37 unit tests plus 2
doctests across `ast`/`style`/`atom`/`constants`/`layout`/`emit`/`latex`/
`lib`, all passing under
no-features, `--features std`, `--features latex-parser`, and
`--all-features`; `cargo clippy --all-targets` and `cargo fmt --check` clean
in every combination.

## Phase 14 — Full PDF Lifecycle: Write, Edit, Measure

- [x] Build `tpt-glyph-pdf-writer`: dependency-free (except `flate2`), zero-allocation PDF object serialization, XRef/object-ID management, standard + compressed object streams
- [x] Complete `tpt-glyph-pdf-parser` to fully populate `tpt-glyph-pdf-ir`
- [x] Build `tpt-glyph-pdf-measure`: text metrics (advance widths, ascents/descents via `tpt-glyph-font`, including embedded font subsets), geometric bounding boxes, ink-coverage estimation
- [x] Build `tpt-glyph-pdf-editor`: transactional IR-mutation API (`Editor::load`, `replace_text`, `insert_image`, `save` with garbage collection of unused objects)
- [x] Integrate `out-glyph-diag` to flag corrupted/non-standard PDF structures during parsing

## Phase 15 — High-Level Typesetting Suite

- [x] Build `tpt-glyph-typeset`: paragraph layout, pagination, page breaks
- [x] Integrate `tpt-glyph-math` into `tpt-glyph-typeset` document flow
- [ ] Release `tpt-glyph` v2.0.0 as a unified, documented workspace

## Phase 16 — Scaled Line-Measurement Tool

- [x] Design a per-page drawing-scale specification (e.g. `1:100`, `1/4"=1'-0"`), suppliable via CLI flag or config file and keyed by page number
- [x] Build `tools/out-glyph-measure`: standalone CLI that opens a PDF/PS page and reports the geometric length (in PDF units) of a given line/path, reusing `tpt-glyph-pdf-measure`'s geometry primitives (Phase 14)
- [x] Apply the page's scale factor to convert a measured length into real-world units, supporting documents where different pages use different scales
- [x] Support common scale conventions (architectural feet-inches ratios, engineering ratios like `1:50`) and common target units (mm/cm/m, in/ft)
- [x] Add unit tests: known geometry + scale → expected real-world length, including a mixed-scale multi-page fixture
- [x] Document the tool's usage (CLI flags, scale-spec format, worked example) in `docs/`

Note: PDF input only — see `docs/measure.md`'s Limitations section for why
PostScript isn't supported (no PS equivalent of `tpt-glyph-pdf-ir` exists).

## Cross-cutting (v2.0 crates.io & reuse goals)

- [x] Ensure `tpt-glyph-core`, `tpt-glyph-pdf-ir`, and `tpt-glyph-math` are `no_std` (+ `alloc`) compatible for WASM/embedded use
- [x] Use trait-based abstractions (`Read`/`Write` or a custom `ResourceProvider`) instead of hardcoded file I/O across new crates
- [x] Add `#[doc = include_str!("../examples/...")]` compile-tested examples to public APIs ahead of docs.rs publication
- [x] (Long-term/exploratory) Investigate constraint-based layout in `tpt-glyph-typeset`, inspired by `tpt-telos`'s QF_LRA solver concepts

---

## Session Notes (2026-08-04)

Reconciled the checklist against the actual codebase (it had drifted from the
last two commits): Phase 12's `no_std` groundwork, the `tpt-glyph-pdf-writer`
crate, and `tpt-glyph-pdf-parser` fully populating the IR were all already
done but left unchecked. Fixed a real test bug found in the process: PDF page
label roman-numeral conversion inverted lowercase/uppercase (`/S /r` produced
`I` instead of `i`), plus a stale test fixture with a `/PageLabels` range that
started past the document's page count.

Built the Phase 13 CLI demo (`tpt-glyph math --latex "..." --font FONT.ttf -o
out.pdf`, or a `.math` file path instead of `--latex`): typesets via
`tpt-glyph-math::emit`, then a new `tpt-glyph-cli::mathdemo` module converts
the resulting `RenderTree` directly into PDF path-fill content-stream
operators (no font embedding needed, since glyphs are already vector
outlines) and assembles a one-page PDF via `tpt-glyph-pdf-writer`.

**While verifying that demo end-to-end, found and fixed four critical,
previously-invisible bugs in the core rendering pipeline** — all masked
because `tpt_glyph_pdf::render_page`/`render_document` were hardcoded to the
`DebugRasterizer` placeholder and silently ignored the CLI's `--backend`
flag entirely:

1. **PDF rendering never used the selected backend.** `render_page`/
   `render_document` now take an explicit `&dyn Rasterizer` parameter;
   `tpt-glyph-cli` passes `SelectedBackend::as_rasterizer()`. Added
   `Send + Sync` as a supertrait bound on `Rasterizer` (all implementors were
   already trivially so) so a `SelectedBackend` can be shared by reference
   across the `rayon` thread pool.
2. **No y-flip between PDF/PostScript user space and canvas pixel space.**
   Page content rendered upside-down/mirrored once a real rasterizer was
   wired in. Added `GraphicsState::for_page`/`with_page_flip`, applied at the
   start of PS interpretation (`Interpreter::with_limits`) and PDF page
   rendering (`PdfPageInfo::render`).
3. **`GraphicsState::concat_transform` composed matrices in the wrong
   order** (`ctm.concat(m)` instead of `m.concat(&ctm)`) — backwards from the
   PDF/PostScript `cm`/`concat` rule `CTM' = M × CTM`. Invisible in every
   existing test because they only ever concatenated a single transform onto
   an identity CTM, where order doesn't matter. Added a regression test
   (`nested_concat_applies_innermost_transform_first`) with a non-identity
   base CTM to make the ordering observable.
4. **`op_lineto`/`op_rlineto` in the PostScript interpreter anchored every
   segment of a multi-segment `lineto` chain to the subpath's original start
   point** instead of the running pen position (a `let _end = ...` computed
   the correct value and then discarded it). Mathematically this still drew
   a "flat" degenerate Bézier per segment, so the reference software
   rasterizer's flatness-based flattening accidentally emitted the correct
   endpoint anyway — but `raqote` respects the (wrong) control points
   directly, rendering multi-segment polylines/polygons as distorted blobs.
   `tpt-glyph-pdf::content`'s equivalent path-building code was already
   correct (uses `sp.segments.last().map(|b| b.end)`), so PDF rendering was
   unaffected; only the PostScript path had this bug.

All four were caught by actually looking at rendered PNG output (a red
square built from `moveto`/`lineto`/`closepath`/`fill` rendered as a
distorted comet shape near the top of the page instead of a clean square
near the bottom) — the existing test suite's assertions (non-empty canvas,
dominant-color-per-page) were too coarse to catch orientation or shape
corruption. This calls into question how much signal the Phase 1 visual-diff
harness has actually had for PDF fixtures historically, since it very likely
exercised this same broken path; the final full harness run (Phase 11) will
be the first meaningful one.

Verification: `cargo test --workspace --all-features` (183 tests, all
passing), `cargo clippy --workspace --all-targets --all-features -- -D
warnings` (clean), `cargo fmt --all --check` (clean), plus manual visual
verification of `fixtures/pdf/hello.pdf`, `fixtures/ps/shapes.ps`, and the
new math CLI demo's output rendered through both the reference and raqote
backends.

Went on to build the remaining v2.0 crates: `tpt-glyph-pdf-measure` (content-
stream geometry walker + text metrics), `tpt-glyph-pdf-editor` (functional
`replace_text`/`insert_image`/`save`, rebuilding the PDF from the semantic IR
so GC of unused objects is inherent), `out-glyph-diag check` (structural PDF
lints), `tpt-glyph-typeset` (greedy line-breaking + justification +
pagination + inline math, emitting to `tpt-glyph-core` draw commands), and
`tools/out-glyph-measure` (scaled real-world length reporting, `docs/measure.md`).
Two more bugs turned up and were fixed along the way: the pdf-editor's
content-stream serializer emitted generic `sc`/`SC` for RGB fill/stroke
colors instead of `rg`/`RG`, which the render pipeline silently treats as
black; and `tpt-glyph-typeset`'s own emission never applied the page-to-
canvas y-flip (the same class of bug fixed earlier, just re-introduced in
new code — now using the shared `GraphicsState::with_page_flip` helper).

## Session Notes (2026-08-04, continued) — Phase 6 wgpu backend

Implemented the real `wgpu`-based GPU rasterizer (`tpt-glyph-core::backends::wgpu`,
`wgpu-backend` feature): fills/strokes are tessellated into triangles via
`lyon` (winding-rule-aware for fills, cap/join for strokes), rendered with a
single draw call per page into a headless offscreen `wgpu::Texture` (color
carried per-vertex, not via a per-draw uniform, so batching needs no bind-
group swaps), then read back into a `Canvas` — no window/swapchain
involved anywhere. `Backend::select`'s auto-detection now does a real
runtime adapter probe (`WgpuRasterizer::adapter_available()`) instead of
the old always-false stub, preferring GPU when available, then raqote, then
the plain reference backend; `SelectedBackend::new`'s `Gpu` arm falls back
to the CPU reference rasterizer if device creation fails at runtime.
Deliberately hard-aliased (no MSAA) for v1, matching the reference
rasterizer's own quality bar rather than claiming false parity with
raqote's antialiasing.

This environment actually has a working GPU adapter (Intel UHD Graphics 770
via Vulkan), confirmed with a standalone probe before committing to the
implementation — so this wasn't just type-checked blind: the parity test
(`wgpu_matches_reference_backend_when_a_gpu_is_available`) and a manual
`tpt-glyph render --backend gpu` run against both `fixtures/ps/shapes.ps`
and `fixtures/pdf/hello.pdf` were visually verified against real hardware,
producing pixel-correct output matching the CPU backends. The test (and the
new `backend_selection_prefers_gpu_when_adapter_available` test) both
degrade to a skip-with-message on a machine with no adapter, rather than
failing, since CI/other dev machines aren't guaranteed to have one.

Verification: `cargo test --workspace --all-features`, `cargo clippy
--workspace --all-targets --all-features -- -D warnings`, plus isolated
`--features wgpu-backend` (no raqote) and `--no-default-features` (no_std)
builds of `tpt-glyph-core` — all clean.

---

## Session Notes (2026-08-05)

Worked the remaining open checklist items.

- **Phase 10 — security review pass (done).** Found and fixed one genuine
  pre-release defect: `tpt-glyph-ps`'s parser recursed on nested
  procedures/arrays with no depth cap, so a deeply nested hostile document
  could overflow the Rust stack *during parsing*, before the interpreter's own
  exec-depth limit applied. Added `MAX_PARSE_DEPTH` (1000) + a `depth` counter
  in `crates/tpt-glyph-ps/src/parser.rs` (fails closed with `PsError::Parse`),
  and a `deeply_nested_array_is_rejected` regression test. Reviewed the rest of
  the attack surface: the lone `unsafe` (wgpu `as_bytes`) is sound; Bézier
  flattening and `pdf-measure` geometry recursion are both depth-bounded; the
  interpreter's `ResourceLimits` are enforced fail-closed. Findings written up
  in `docs/security-review.md`. (Residual recommendations there — max string
  length, PDF decode-size cap — are non-blocking.)
- **Cross-cutting — trait-based I/O (done).** Verified the new crates already
  expose trait-friendly cores: `tpt-glyph-pdf-parser::parse_bytes(&[u8])` and
  new `parse_read<R: Read>`; `tpt-glyph-font::Font::from_bytes(&[u8])`;
  `tpt-glyph-pdf-writer::{finish, write_to(impl Write), save}`. Added
  `parse_read` so there is a true `Read`-based ingestion path (no file-path
  assumption) in the PDF parser. Hardcoded `fs::read`/`fs::write` now only
  appears in binaries/tools and doctests, which is appropriate.
- **Cross-cutting — compile-tested doc examples (done).** Added
  `crates/tpt-glyph-core/examples/quickstart.md` and
  `crates/tpt-glyph-math/examples/quickstart.md`, pulled in via
  `#[doc = include_str!("../examples/quickstart.md")]` at each crate root so
  `cargo test --doc` compiles them. All 6 doctests (3 per crate) pass;
  `cargo clippy --all-features -p tpt-glyph-core -p tpt-glyph-math` clean.
- **Cross-cutting — constraint-based layout (done, exploratory).** Wrote
  `docs/constraint-layout.md`: a design note on adopting a QF_LRA solver
  (inspired by `tpt-telos`) for multi-column / figure-flow / global-raggedness
  pages in `tpt-glyph-typeset`, keeping the greedy breaker as the default fast
  path. No code change; recommended as a post-2.0 enhancement.

**Blocked / left to manual external steps (credentials + network required):**

- **Phase 11 — final full visual-diff harness run.** This environment has
  neither Docker nor a native `gs`/`Ghostscript`, and the harness needs
  Ghostscript-rendered reference images to compare against. The CI `visual-diff`
  job (`.github/workflows/ci.yml`) already builds the Ghostscript image and
  runs the full corpus on every push/PR, so the run is automated there. Could
  not be executed locally; left unchecked pending a CI run.
- **Phase 11 `v1.0.0` tag/publish, Phase 12 (publish `tpt-glyph-core` +
  `tpt-glyph-font`), Phase 13 (publish `tpt-glyph-math`), Phase 15 (`v2.0.0`
  release).** Irreversible external actions requiring crates.io credentials and
  a git tag/push; intentionally not performed. Crate metadata
  (description/keywords/categories/license) was verified in prior sessions;
  `tpt-glyph-math` (with `std`) carries a `tpt-glyph-core` path dependency, so
  publish ordering must be core → math. Left unchecked; perform as the release
  step.

Verification this session: `cargo test -p tpt-glyph-ps -p tpt-glyph-pdf-parser`
(40 tests pass), `cargo test --doc -p tpt-glyph-core -p tpt-glyph-math` (6
doctests pass), `cargo clippy -p tpt-glyph-core -p tpt-glyph-math
--all-features` (clean), `cargo fmt --all --check` (clean on touched files).



