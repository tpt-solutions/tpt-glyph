#!/usr/bin/env powershell
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# TPT Glyph visual-diff harness runner.
#
# Regenerates Ghostscript reference renders for the fixture corpus (requires
# Docker) and runs tools/glyph-diff to compare against TPT Glyph candidate
# output. When Ghostscript/Docker is unavailable, references are treated as
# "pending" so the tooling itself remains exercisable in CI.
#
# Usage:
#   .\tools\run-diff.ps1 [-Dpi 72] [-ReferenceDir fixtures\reference] [-CandidateDir fixtures\candidate]

param(
    [int]$Dpi = 72,
    [string]$ReferenceDir = "fixtures/reference",
    [string]$CandidateDir = "fixtures/candidate",
    [string]$Thresholds = "fixtures/thresholds.json",
    [string]$Report = "fixtures/diff-report.json",
    [switch]$FailMissingReference
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")

# 1. Generate reference renders with Ghostscript in Docker, if available.
$docker = Get-Command docker -ErrorAction SilentlyContinue
if ($docker) {
    Write-Host "Rendering Ghostscript references (DPI=$Dpi)..."
    docker build -f docker/ghostscript/Dockerfile -t glyph-gs "$root/docker/ghostscript" | Out-Null
    foreach ($f in Get-ChildItem "$root/fixtures/ps" -Filter *.ps) {
        $name = $f.BaseName
        docker run --rm `
            -v "$root/fixtures:/work" glyph-gs `
            -dNOPAUSE -dBATCH -sDEVICE=png16m "-r$Dpi" `
            "-sOutputFile=/work/reference/$name.png" "/work/ps/$($f.Name)" | Out-Null
    }
} else {
    Write-Warning "docker not found: skipping Ghostscript reference generation. References will be 'pending'."
}

# 2. Run the diff harness.
$missing = if ($FailMissingReference) { "fail" } else { "pending" }
cargo run -q --manifest-path "$root/Cargo.toml" --bin glyph-diff -- `
    --reference $ReferenceDir --candidate $CandidateDir `
    --thresholds $Thresholds --report $Report `
    --missing-reference $missing
