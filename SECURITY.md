# Security Policy

TPT Glyph is a secure, sandboxed, multi-threaded PDF/PostScript rendering engine.
Untrusted input is a first-class concern: a renderer is, by design, handed
attacker-controlled documents. This document describes how to report
vulnerabilities and summarizes the threat model and the defenses already in
place.

## Reporting a Vulnerability

Please report security vulnerabilities **privately**. Do not open a public issue.

- Email: **security@tpt-solutions.example** (replace with the real address before
  publishing)
- Alternatively, use GitHub's private vulnerability reporting on the
  [repository](https://github.com/tpt-solutions/tpt-glyph).

Include:

- A description of the impact and a minimal reproduction (sample file or input).
- The affected crate/version and platform.
- Any crash backtrace or sanitizer output.

We aim to acknowledge reports within **5 business days** and to provide a
remediation timeline within **15 business days**. Credit will be given to
reporters who wish to be named.

## Supported Versions

Only the latest released `0.x` / `1.x` line receives security fixes. See
`CHANGELOG.md` for the current supported version.

## Threat Model

TPT Glyph processes **untrusted** PDF and PostScript documents. The primary
adversary is someone who can supply a document to be rendered (e.g. via a web
upload, email attachment, or print service).

### Assets

- The host process's memory, filesystem, and network.
- Other documents/pages being rendered concurrently in the same process.
- The correctness and availability of the rendering service (no denial of service).

### Attack Surface

| Surface | Risk |
| --- | --- |
| PDF/PostScript parser | Malformed structure → parser confusion, OOM, hangs. |
| Content-stream interpreter (PostScript) | Unbounded loops/recursion → CPU/stack exhaustion; arbitrary stack growth. |
| Rasterizer | Pathological geometry (huge coordinates, deep Bézier subdivision) → OOM/hangs. |
| Font/XObject decoding | Decompression bombs in Flate/object streams → memory exhaustion. |
| Output | Unbounded number of pages or output pixels → disk/CPU exhaustion. |

### Trusted Boundary

The document is **untrusted**. The engine is **trusted** and must fail closed:
malformed input must produce an error, never undefined behavior, and never
escape the process.

## Defenses in Place (Phase 10)

- **Immutable graphics state.** `GraphicsState` is a value type with no shared
  mutable references, so concurrent per-page rendering cannot corrupt each
  other's state (no cross-page state leakage).
- **Resource limits.** The PostScript interpreter enforces configurable bounds
  on operand-stack size, execution-stack (recursion) depth, emitted draw
  commands (output size), and total instructions executed. See
  `glyph_ps::ResourceLimits` (`strict()` for untrusted input).
- **Deterministic rasterizer.** The reference software rasterizer is pure and
  holds no global mutable state, so it is safe under rayon concurrency.
- **Fuzzing.** `cargo-fuzz` targets exercise the PostScript interpreter and the
  PDF content-stream path (see `fuzz/`).
- **Knowledge-graph coverage gate.** The interpreter's dispatch table is derived
  from the Phase 3 knowledge graph; unknown/unimplemented operators are reported
  rather than silently mishandled.

## Recommended Hardening for Deployers

- Run the `glyph` CLI / library inside an OS-level sandbox (seccomp, landlock,
  or a dedicated low-privilege container) with no network access and a small
  tmpfs for outputs.
- Always pass `ResourceLimits::strict()` (or tighter) when rendering untrusted
  documents.
- Bound total wall-clock time and output size at the orchestration layer in
  addition to in-engine limits.
