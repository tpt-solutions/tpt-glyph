// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — out-glyph-measure CLI
//
// Thin CLI wrapper around the `tpt-glyph-pdf-measure` library: parses scale
// options, opens the requested PDF page, and prints measured path lengths.

use clap::Parser;
use out_glyph_measure::{measure_page, measure_path, LengthUnit, ScaleSpec, ScaleTable};

#[derive(Parser)]
#[command(
    name = "out-glyph-measure",
    version,
    about = "Report real-world lengths of PDF page geometry under a drawing scale"
)]
struct Cli {
    /// Input PDF path.
    input: std::path::PathBuf,

    /// 1-based page number to measure.
    #[arg(long, default_value_t = 1)]
    page: usize,

    /// Measure only this painted-path index (0-based, as listed when this
    /// flag is omitted).
    #[arg(long)]
    path_index: Option<usize>,

    /// Default drawing scale applied to any page without its own override,
    /// e.g. "1:100" (ratio) or "1/4in=1ft" (architectural).
    #[arg(long)]
    scale: Option<String>,

    /// Per-page scale override, repeatable: "PAGE=SPEC" (e.g. "3=1:50").
    #[arg(long = "page-scale", value_name = "PAGE=SPEC")]
    page_scale: Vec<String>,

    /// Load a scale config from JSON: {"default": "1:100", "pages": {"1": "1/4in=1ft"}}.
    /// Combined with `--scale`/`--page-scale`, which take precedence.
    #[arg(long)]
    scale_file: Option<std::path::PathBuf>,

    /// Output unit for real-world lengths: mm, cm, m, in, or ft.
    #[arg(long, default_value = "mm")]
    unit: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut table = match &cli.scale_file {
        Some(path) => {
            let json = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
            ScaleTable::from_json(&json).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?
        }
        None => ScaleTable::new(),
    };
    if let Some(spec) = &cli.scale {
        table = table.with_default(ScaleSpec::parse(spec).map_err(|e| anyhow::anyhow!("{e}"))?);
    }
    for entry in &cli.page_scale {
        let (page, spec) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--page-scale expects PAGE=SPEC, got {entry:?}"))?;
        let page: usize = page
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("bad page number in --page-scale: {entry:?}"))?;
        table = table.with_page(
            page,
            ScaleSpec::parse(spec).map_err(|e| anyhow::anyhow!("{e}"))?,
        );
    }

    let unit = LengthUnit::parse(&cli.unit)
        .ok_or_else(|| anyhow::anyhow!("unknown unit: {}", cli.unit))?;

    let doc = tpt_glyph_pdf_parser::parse_path(&cli.input)
        .map_err(|e| anyhow::anyhow!("failed to open {}: {e}", cli.input.display()))?;
    let page_index0 = cli
        .page
        .checked_sub(1)
        .ok_or_else(|| anyhow::anyhow!("--page must be >= 1"))?;
    let page = doc.page(page_index0).ok_or_else(|| {
        anyhow::anyhow!(
            "{} has {} page(s); page {} does not exist",
            cli.input.display(),
            doc.page_count(),
            cli.page
        )
    })?;
    let scale = table.scale_for(cli.page);

    match cli.path_index {
        Some(idx) => {
            let m = measure_path(page, idx, scale).ok_or_else(|| {
                anyhow::anyhow!("page {} has no painted path at index {idx}", cli.page)
            })?;
            print_measurement(&m, unit);
        }
        None => {
            let measurements = measure_page(page, scale);
            if measurements.is_empty() {
                println!("page {} has no painted paths", cli.page);
            }
            for m in &measurements {
                print_measurement(m, unit);
            }
        }
    }
    Ok(())
}

fn print_measurement(m: &out_glyph_measure::Measurement, unit: LengthUnit) {
    println!(
        "path {}: {:?}, {:.3} pdf units -> {:.3}{unit}",
        m.path_index,
        m.kind,
        m.pdf_length,
        unit.from_mm(m.real_world_mm)
    );
}
