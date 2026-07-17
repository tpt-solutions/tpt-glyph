# Visual-Diff Testing Harness

TPT Glyph is validated against upstream **Ghostscript** by pixel-comparing its
output to Ghostscript's reference renders across a fixture corpus. This document
explains how the harness is organized and how to use it.

## Layout

```
docker/ghostscript/   Dockerfile building upstream Ghostscript for reference renders
fixtures/
  ps/                 PostScript fixture inputs (varied complexity)
  pdf/                PDF fixture inputs (added in Phase 5)
  reference/          Ghostscript-produced reference PNGs (generated)
  candidate/          TPT Glyph-produced candidate PNGs (generated)
  thresholds.json     Pass/fail thresholds
  diff-report.json    Latest JSON report (generated)
tools/
  glyph-diff/         Pixel-diff comparison crate (MSE, peak error, SSIM)
  run-diff.ps1        Local runner: generates references + runs glyph-diff
```

## Metrics & thresholds

`tools/glyph-diff` compares two equal-sized RGBA images and reports:

| Metric | Meaning | Default threshold |
|--------|---------|-------------------|
| `mse` | Mean squared error over RGB channels, scaled 0..1 | `max_mse ≤ 0.01` |
| `peak_error` | Largest single-channel absolute difference, 0..255 | `max_peak_error ≤ 24` |
| `ssim` | 8×8-block mean structural similarity, 0..1 | `min_ssim ≥ 0.98` |

A fixture **passes** only when all three thresholds are satisfied. Thresholds
live in `fixtures/thresholds.json` and can be tuned as the engine matures.

## Running locally

```sh
# 1. Generate Ghostscript references (requires Docker):
docker build -f docker/ghostscript/Dockerfile -t glyph-gs docker/ghostscript
for f in fixtures/ps/*.ps; do
  name=$(basename "$f" .ps)
  docker run --rm -v "$PWD/fixtures:/work" glyph-gs \
    -dNOPAUSE -dBATCH -sDEVICE=png16m -r72 \
    "-sOutputFile=/work/reference/$name.png" "/work/ps/$name.ps"
done

# 2. Once TPT Glyph can render, drop candidates into fixtures/candidate/ and:
cargo run -p glyph-diff -- \
  --reference fixtures/reference --candidate fixtures/candidate \
  --thresholds fixtures/thresholds.json --report fixtures/diff-report.json \
  --missing-reference pending
```

On Windows, `tools/run-diff.ps1` automates both steps (it skips reference
generation with a warning if Docker is unavailable).

## CI

The `visual-diff` job in `.github/workflows/ci.yml` builds the Ghostscript image,
renders references for all `fixtures/ps/*.ps`, and runs `glyph-diff`. Until
candidate rendering exists, missing references are reported as **pending**
(non-fatal). Once candidate output is wired in, switch the runner to
`--missing-reference fail` to make visual regressions a hard CI gate.

## Adding a fixture

Drop a new `.ps` (or later `.pdf`) file into `fixtures/ps/` (resp. `fixtures/pdf/`).
It is automatically picked up by both the reference renderer and the diff harness
on the next run — no code change required.
