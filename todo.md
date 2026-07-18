# TPT Glyph — Project Checklist

TPT Glyph: a secure, sandboxed, multi-threaded PDF/PostScript rendering engine written in Rust — the Ghostscript successor. Built by TPT Solutions, dual-licensed under MIT OR Apache-2.0.

---

## Phase 0 — Project Setup & Licensing

- [ ] Initialize Cargo workspace (`tpt-glyph`) with member crates
- [ ] Create crate layout: `crates/glyph-core` (engine lib), `crates/glyph-cli` (binary), `crates/glyph-kg` (knowledge graph), `crates/glyph-diag` (AI diagnostic tool)
- [ ] Add `LICENSE-MIT` (Copyright TPT Solutions)
- [ ] Add `LICENSE-APACHE` (Copyright TPT Solutions)
- [ ] Add `license = "MIT OR Apache-2.0"` to all crate `Cargo.toml` files
- [ ] Add SPDX dual-license header convention for source files
- [ ] Write root `README.md` skeleton (purpose, architecture overview, status)
- [ ] Set up `.gitignore`, `rustfmt.toml`, `clippy.toml`
- [ ] Initialize git repository, initial commit
- [ ] Set up CI pipeline skeleton (build + test on push/PR)
- [ ] Choose MSRV (minimum supported Rust version) and pin in CI

## Phase 1 — Ghostscript Diff Testing Harness (early, parallel to core work)

- [ ] Write Dockerfile that builds/runs upstream Ghostscript for reference rendering
- [ ] Build fixture corpus: sample PDF files (varied complexity) and PostScript files
- [ ] Write pixel-diff comparison tool (compare TPT Glyph output vs Ghostscript output)
- [ ] Define pass/fail thresholds for visual diffing (pixel tolerance, SSIM, etc.)
- [ ] Wire visual-diff harness into CI as a regression gate
- [ ] Add script/tooling to add new fixtures easily as coverage grows
- [ ] Document harness usage in `CONTRIBUTING.md` or `docs/testing.md`

## Phase 2 — Core Architecture: Graphics State & Rendering Pipeline

- [ ] Design immutable `GraphicsState` context struct (color, line width, transform matrices, clip path)
- [ ] Define rendering tree / traversal model (how operators produce draw commands)
- [ ] Define core path/geometry primitives (points, subpaths, beziers)
- [ ] Define pixel buffer / canvas abstraction (backend-agnostic)
- [ ] Define `Page` and `Document` abstractions
- [ ] Design concurrency model for per-page parallel rendering (e.g. `rayon` thread pool)
- [ ] Write unit tests proving `GraphicsState` is immutable/no shared mutable state
- [ ] Document the architecture decision (why immutable context vs Ghostscript's globals)

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

Phase 5 complete: `glyph-pdf` opens PDFs via the `pdf` crate, walks the page
tree, decodes content-stream operators onto the shared glyph-core pipeline
(path construction/painting, CTM, color, line attributes, ExtGState), and
implements PDF-specific text positioning (`Tf`/`Tm`/`Td`/`Tj`/`TJ`) plus XObject
(Form + Image) handling. Baseline font/string decoding (PDFDocEncoding,
UTF-16BE) and glyph-advance lookup are in place; glyph outlines are stamped as
placeholder boxes. The `hello.pdf` fixture renders end-to-end through the CLI
and the Phase 1 harness now renders PDF candidates.

## Phase 6 — Rasterization Backends

- [ ] Implement `wgpu`-based GPU rendering backend (primary path)
- [ ] Implement `raqote`-based CPU rendering backend (fallback path)
- [ ] Implement backend abstraction/trait so pipeline is backend-agnostic
- [ ] Implement runtime backend selection/detection (GPU available vs fallback)
- [ ] Write parity tests: same document renders equivalently on both backends (via Phase 1 harness)
- [ ] Benchmark both backends on the fixture corpus

## Phase 7 — Multi-threaded Multi-page Rendering

- [ ] Wire concurrent per-page rendering using immutable `GraphicsState` context across threads
- [ ] Add stress tests rendering large multi-page PDFs concurrently
- [ ] Add correctness tests proving no cross-page state leakage/races (e.g. with loom or targeted stress tests)
- [ ] Benchmark multi-threaded rendering throughput vs single-threaded Ghostscript baseline
- [ ] Tune thread pool sizing / work-stealing strategy

## Phase 8 — CLI & Library Crate Polish

- [ ] Design public `glyph-core` library API (documents, render options, output targets)
- [ ] Implement `glyph` CLI binary: render/convert commands (PDF/PS → PNG/other raster formats)
- [ ] Add CLI options mirroring common Ghostscript flags where sensible (resolution, page range, output format)
- [ ] Write `rustdoc` documentation and usage examples for the library crate
- [ ] Write CLI usage docs / `--help` output polish
- [ ] Add integration tests for the CLI binary

## Phase 9 — AI-Assisted Diagnostic Tool

- [ ] Design `glyph-diag` companion tool consuming the Phase 3 knowledge graph
- [ ] Implement operator coverage reporting (which PS/PDF operators are implemented vs missing)
- [ ] Implement Ghostscript-diff-driven analysis (surface which fixtures fail and why, using Phase 1 harness output)
- [ ] Implement AI/LLM-assisted fix suggestion feature (propose likely causes/fixes from diff + KG context)
- [ ] Build developer-facing CLI (or lightweight UI) for exploring the graph and diagnostics
- [ ] Document how to use the diagnostic tool during development

## Phase 10 — Sandboxing & Security Hardening

- [ ] Threat-model untrusted PDF/PS input handling
- [ ] Implement sandboxed execution for untrusted documents (process isolation / capability restrictions)
- [ ] Add resource-limit enforcement (memory, CPU time, recursion depth, output size)
- [ ] Set up fuzzing (e.g. `cargo-fuzz`) targeting parser and interpreter
- [ ] Run and triage fuzzing results; fix discovered crashes/hangs
- [ ] Conduct a security review pass before release
- [ ] Add `SECURITY.md` with vulnerability reporting process

## Phase 11 — Release Prep

- [ ] Finalize `README.md` with full architecture overview and usage examples
- [ ] Write `CHANGELOG.md`
- [ ] Decide on versioning scheme (SemVer) and tag v1.0.0 criteria
- [ ] Verify crate metadata (description, keywords, categories, license) for publishing
- [ ] Confirm dual MIT/Apache-2.0 licensing is correctly applied across all crates and files
- [ ] Final full run of Phase 1 visual-diff harness across entire fixture corpus
- [ ] Tag and publish v1.0.0 release
