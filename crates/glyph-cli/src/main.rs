// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-cli
//
// Command-line front-end. Currently a skeleton; rendering commands are wired in
// later phases.

use clap::{Parser, Subcommand};
use glyph_core::render::Rasterizer;

#[derive(Parser)]
#[command(
    name = "glyph",
    version,
    about = "TPT Glyph — secure multi-threaded PDF/PostScript renderer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a PDF or PostScript document to raster images.
    Render {
        /// Input document (.pdf or .ps).
        input: std::path::PathBuf,
        /// Output directory for rendered pages.
        output: std::path::PathBuf,
        /// Resolution in DPI (default 72).
        #[arg(long, default_value_t = 72)]
        dpi: u32,
        /// Page range (e.g. "1-3"). Defaults to all pages.
        #[arg(long)]
        pages: Option<String>,
    },
    /// Print version and build information.
    Version,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Render {
            input,
            output,
            dpi,
            pages,
        } => {
            std::fs::create_dir_all(&output)?;
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".into());

            let is_pdf = input
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false);

            if is_pdf {
                let doc = glyph_pdf::PdfDocument::open_path(&input)
                    .map_err(|e| anyhow::anyhow!("failed to open PDF {}: {e}", input.display()))?;
                let indices = resolve_page_range(doc.page_count(), pages.as_deref())?;
                let mut rendered = 0usize;
                for idx in indices {
                    let canvas = glyph_pdf::render_page(
                        &doc,
                        idx,
                        glyph_core::graphics_state::GraphicsState::new(),
                    )
                    .map_err(|e| anyhow::anyhow!("render error: {e}"))?;
                    let out_path = output.join(format!("{stem}-{}.png", idx + 1));
                    canvas.save_png(&out_path).map_err(|e| {
                        anyhow::anyhow!("failed to write {}: {e}", out_path.display())
                    })?;
                    println!(
                        "rendered {} page {} -> {}",
                        input.display(),
                        idx + 1,
                        out_path.display()
                    );
                    rendered += 1;
                }
                println!("rendered {rendered} page(s) of {}", input.display());
                Ok(())
            } else {
                // Default PostScript page size at the given DPI (US Letter, 612x792 pt).
                // Device pixels = points * dpi / 72.
                let scale = dpi as f64 / 72.0;
                let width = (612.0 * scale).round() as u32;
                let height = (792.0 * scale).round() as u32;

                let src = std::fs::read_to_string(&input)
                    .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", input.display()))?;

                // Render the single page produced by the PostScript program.
                let mut interp = glyph_ps::Interpreter::new(width, height);
                interp
                    .run_source(&src)
                    .map_err(|e| anyhow::anyhow!("interpreter error: {e}"))?;

                let tree = interp.tree();
                let canvas = glyph_core::render::DebugRasterizer
                    .rasterize(tree)
                    .map_err(|e| anyhow::anyhow!("rasterize error: {e}"))?;

                let page_label = pages.clone().unwrap_or_else(|| "1".to_string());
                let out_path = output.join(format!("{stem}-{page_label}.png"));
                canvas
                    .save_png(&out_path)
                    .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", out_path.display()))?;

                println!(
                    "rendered {} -> {} ({}x{}, dpi={})",
                    input.display(),
                    out_path.display(),
                    width,
                    height,
                    dpi
                );
                Ok(())
            }
        }
        Command::Version => {
            println!("glyph {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

/// Resolve a page-range string (e.g. "1-3", "2") into 0-based page indices.
fn resolve_page_range(total: usize, range: Option<&str>) -> anyhow::Result<Vec<usize>> {
    let total = total.max(1);
    match range {
        None => Ok((0..total).collect()),
        Some(s) => {
            let mut out = Vec::new();
            for part in s.split(',') {
                let part = part.trim();
                if let Some((a, b)) = part.split_once('-') {
                    let a: usize = a
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("bad page range: {s}"))?;
                    let b: usize = b
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("bad page range: {s}"))?;
                    for p in a..=b {
                        if p >= 1 && p <= total {
                            out.push(p - 1);
                        }
                    }
                } else {
                    let p: usize = part.parse().map_err(|_| anyhow::anyhow!("bad page: {s}"))?;
                    if p >= 1 && p <= total {
                        out.push(p - 1);
                    }
                }
            }
            if out.is_empty() {
                anyhow::bail!("no pages matched range '{s}' (document has {total} pages)");
            }
            Ok(out)
        }
    }
}
