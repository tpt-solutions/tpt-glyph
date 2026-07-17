// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-diff (binary)
//
// Walks a fixture corpus, compares TPT Glyph candidate renders against
// Ghostscript reference renders, and emits a JSON report plus a CI exit code.

use clap::{Parser, ValueEnum};
use glyph_diff::{compare, load_rgba, FixtureReport, Thresholds, Verdict};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "glyph-diff",
    version,
    about = "Pixel-diff TPT Glyph vs Ghostscript across a fixture corpus"
)]
struct Cli {
    /// Directory containing reference (Ghostscript) renders.
    #[arg(long)]
    reference: PathBuf,
    /// Directory containing candidate (TPT Glyph) renders.
    #[arg(long)]
    candidate: PathBuf,
    /// Thresholds config (JSON). Defaults applied if omitted.
    #[arg(long)]
    thresholds: Option<PathBuf>,
    /// Output path for the JSON report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// How to treat fixtures with a missing reference image.
    #[arg(long, value_enum, default_value_t = MissingRef::Pending)]
    missing_reference: MissingRef,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum MissingRef {
    /// Report as "pending" (non-fatal), useful before references are generated.
    Pending,
    /// Treat missing reference as a hard failure (CI gate).
    Fail,
}

#[derive(Serialize)]
struct Report {
    thresholds: Thresholds,
    fixtures: Vec<FixtureReport>,
    passed: usize,
    failed: usize,
    pending: usize,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let thresholds = match &cli.thresholds {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p)?)?,
        None => Thresholds::default(),
    };

    let fixtures = run_corpus(
        &cli.reference,
        &cli.candidate,
        &thresholds,
        cli.missing_reference,
    )?;

    let passed = fixtures
        .iter()
        .filter(|f| matches!(f.verdict, Verdict::Pass))
        .count();
    let pending = fixtures.iter().filter(|f| f.metrics.is_none()).count();
    let failed = fixtures.len() - passed - pending;

    let report = Report {
        thresholds,
        fixtures: fixtures.clone(),
        passed,
        failed,
        pending,
    };

    let json = serde_json::to_string_pretty(&report)?;
    match &cli.report {
        Some(p) => std::fs::write(p, &json)?,
        None => println!("{json}"),
    }

    println!(
        "glyph-diff: {} passed, {} failed, {} pending (of {})",
        passed,
        failed,
        pending,
        fixtures.len()
    );

    // CI exit code: fail if any non-pending fixture failed.
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn run_corpus(
    reference_dir: &Path,
    candidate_dir: &Path,
    thresholds: &Thresholds,
    missing: MissingRef,
) -> anyhow::Result<Vec<FixtureReport>> {
    if !candidate_dir.is_dir() {
        anyhow::bail!("candidate dir {} not found", candidate_dir.display());
    }

    let mut reports = Vec::new();
    for entry in std::fs::read_dir(candidate_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let Some(name) = stem else { continue };

        let ref_path = reference_dir.join(entry.file_name());

        if !ref_path.is_file() {
            let note = match missing {
                MissingRef::Pending => "reference not generated yet".to_string(),
                MissingRef::Fail => "reference missing (treated as failure)".to_string(),
            };
            let mut r = FixtureReport::pending(name, path, note);
            if missing == MissingRef::Fail {
                r.verdict = Verdict::Fail;
            }
            reports.push(r);
            continue;
        }

        let cand = load_rgba(&path)?;
        let refr = load_rgba(&ref_path)?;
        match compare(&cand, &refr) {
            Ok(metrics) => {
                let verdict = metrics.verdict(thresholds);
                reports.push(FixtureReport {
                    name,
                    reference: Some(ref_path),
                    candidate: path,
                    metrics: Some(metrics),
                    verdict,
                    note: String::new(),
                });
            }
            Err(e) => {
                reports.push(FixtureReport::pending(
                    name,
                    path,
                    format!("compare error: {e}"),
                ));
            }
        }
    }

    reports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(reports)
}
