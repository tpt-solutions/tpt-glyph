# SPDX-License-Identifier: MIT OR Apache-2.0
#
# TPT Glyph — visual-diff local runner
#
# Generates Ghostscript reference renders (requires Docker) and TPT Glyph
# candidate renders for every fixture in fixtures/ps/*.ps and fixtures/pdf/*.pdf,
# then runs glyph-diff. Mirrors the CI visual-diff job for local development.
#
# Usage:
#   pwsh tools/run-diff.ps1
#   pwsh tools/run-diff.ps1 -SkipReferences   # reuse existing references

[CmdletBinding()]
param(
    [switch]$SkipReferences,
    [int]$Dpi = 72,
    [string]$MissingReference = "pending"  # pending | fail
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$fixtures = Join-Path $root "fixtures"
$referenceDir = Join-Path $fixtures "reference"
$candidateDir = Join-Path $fixtures "candidate"
New-Item -ItemType Directory -Force -Path $referenceDir, $candidateDir | Out-Null

$cargo = "cargo"

# --- 1. Ghostscript reference renders --------------------------------------
if (-not $SkipReferences) {
    $docker = Get-Command docker -ErrorAction SilentlyContinue
    if (-not $docker) {
        Write-Warning "Docker not found; skipping Ghostscript reference generation. " +
                      "Reuse existing references or install Docker."
    } else {
        Write-Host "Building Ghostscript image..."
        docker build -f (Join-Path $root "docker/ghostscript/Dockerfile") -t glyph-gs (Join-Path $root "docker/ghostscript") | Out-Null

        foreach ($f in Get-ChildItem (Join-Path $fixtures "ps") -Filter *.ps) {
            $name = $f.BaseName
            Write-Host "Ghostscript: $name.ps -> reference/$name.png"
            docker run --rm -v "${fixtures}:/work" glyph-gs `
                -dNOPAUSE -dBATCH -sDEVICE=png16m -r$Dpi `
                --permit-file-read=/work/* --permit-file-write=/work/* `
                "-sOutputFile=/work/reference/$name.png" "/work/ps/$name.ps" | Out-Null
        }

        foreach ($f in Get-ChildItem (Join-Path $fixtures "pdf") -Filter *.pdf) {
            $name = $f.BaseName
            Write-Host "Ghostscript: $name.pdf -> reference/$name.png"
            docker run --rm -v "${fixtures}:/work" glyph-gs `
                -dNOPAUSE -dBATCH -sDEVICE=png16m -r$Dpi `
                --permit-file-read=/work/* --permit-file-write=/work/* `
                "-sOutputFile=/work/reference/$name.png" "/work/pdf/$name.pdf" | Out-Null
        }
    }
}

# --- 2. TPT Glyph candidate renders ----------------------------------------
foreach ($f in Get-ChildItem (Join-Path $fixtures "ps") -Filter *.ps) {
    $name = $f.BaseName
    $out = Join-Path $candidateDir "$name.png"
    Write-Host "TPT Glyph: $name.ps -> candidate/$name.png"
    & $cargo run -q -p glyph-cli -- render $f.FullName $candidateDir --dpi $Dpi | Out-Null
}

foreach ($f in Get-ChildItem (Join-Path $fixtures "pdf") -Filter *.pdf) {
    $name = $f.BaseName
    $out = Join-Path $candidateDir "$name.png"
    Write-Host "TPT Glyph: $name.pdf -> candidate/$name.png"
    & $cargo run -q -p glyph-cli -- render $f.FullName $candidateDir --dpi $Dpi | Out-Null
}

# --- 3. Run the diff harness ------------------------------------------------
& $cargo run -q -p glyph-diff -- `
    --reference $referenceDir `
    --candidate $candidateDir `
    --thresholds (Join-Path $fixtures "thresholds.json") `
    --report (Join-Path $fixtures "diff-report.json") `
    --missing-reference $MissingReference
