// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-cli
//
// Command-line front-end. Currently a skeleton; rendering commands are wired in
// later phases.

use clap::{Parser, Subcommand, ValueEnum};
use tpt_glyph_core::backend::Backend;

#[derive(Parser)]
#[command(
    name = "tpt-glyph",
    version,
    about = "TPT Glyph — secure multi-threaded PDF/PostScript renderer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Rasterization backend selection exposed on the command line.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Default)]
enum BackendChoice {
    /// Pick the best available backend at runtime.
    #[default]
    Auto,
    /// Reference software scanline rasterizer (always available).
    Cpu,
    /// raqote CPU rasterizer (antialiased; compiled into this build).
    CpuRaqote,
    /// GPU rasterizer via wgpu (currently falls back to CPU).
    Gpu,
}

impl From<BackendChoice> for Option<Backend> {
    fn from(c: BackendChoice) -> Self {
        match c {
            BackendChoice::Auto => None,
            BackendChoice::Cpu => Some(Backend::Cpu),
            BackendChoice::CpuRaqote => Some(Backend::CpuRaqote),
            BackendChoice::Gpu => Some(Backend::Gpu),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Render a PDF or PostScript document to PNG raster images.
    ///
    /// Pages are rasterized through the backend-agnostic tpt-glyph-core pipeline.
    /// PDFs are parsed and rendered per-page; PostScript programs are executed
    /// by the interpreter and their emitted draw commands rasterized. With
    /// `--parallel`, pages are rendered concurrently across a rayon thread pool
    /// (each page is independent, so this is data-race free by construction).
    Render {
        /// Input document (.pdf or .ps).
        input: std::path::PathBuf,
        /// Output directory for rendered pages (created if missing).
        output: std::path::PathBuf,
        /// Resolution in DPI. Default 72. Higher values produce larger images.
        #[arg(long, default_value_t = 72)]
        dpi: u32,
        /// Page range to render, e.g. "1-3" or "1,3,5". Defaults to all pages.
        #[arg(long)]
        pages: Option<String>,
        /// Rasterization backend: `auto` (best available), `cpu` (reference
        /// scanline), `cpuraqote` (raqote CPU), or `gpu` (resolves to CPU until
        /// the wgpu path lands).
        #[arg(long, value_enum, default_value_t = BackendChoice::Auto)]
        backend: BackendChoice,
        /// Render pages concurrently across the rayon thread pool.
        #[arg(long)]
        parallel: bool,
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
            backend,
            parallel,
        } => {
            std::fs::create_dir_all(&output)?;
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".into());

            let backend = tpt_glyph_core::backend::SelectedBackend::auto(backend.into());
            println!("using backend: {:?}", backend.kind());

            let is_pdf = input
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false);

            if is_pdf {
                let doc = tpt_glyph_pdf::PdfDocument::open_path(&input)
                    .map_err(|e| anyhow::anyhow!("failed to open PDF {}: {e}", input.display()))?;
                let indices = resolve_page_range(doc.page_count(), pages.as_deref())?;

                // Phase 7: render the requested pages concurrently across the
                // rayon thread pool, then write them out. The per-page render is
                // independent and state-free, so this is data-race free.
                // PDF pages are already rasterized by tpt-glyph-pdf into Canvases.
                let canvases: Vec<(usize, tpt_glyph_core::canvas::Canvas)> = if parallel {
                    use rayon::prelude::*;
                    indices
                        .par_iter()
                        .map(|&idx| {
                            let canvas = tpt_glyph_pdf::render_page(
                                &doc,
                                idx,
                                tpt_glyph_core::graphics_state::GraphicsState::new(),
                            )
                            .map_err(|e| {
                                anyhow::anyhow!("render error on page {}: {e}", idx + 1)
                            })?;
                            Ok::<_, anyhow::Error>((idx, canvas))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    indices
                        .iter()
                        .map(|&idx| {
                            let canvas = tpt_glyph_pdf::render_page(
                                &doc,
                                idx,
                                tpt_glyph_core::graphics_state::GraphicsState::new(),
                            )
                            .map_err(|e| {
                                anyhow::anyhow!("render error on page {}: {e}", idx + 1)
                            })?;
                            Ok::<_, anyhow::Error>((idx, canvas))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };

                let mut rendered = 0usize;
                for (idx, canvas) in canvases {
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
                let mut interp = tpt_glyph_ps::Interpreter::new(width, height);
                interp
                    .run_source(&src)
                    .map_err(|e| anyhow::anyhow!("interpreter error: {e}"))?;

                let tree = interp.tree();
                let canvas = backend
                    .rasterize(tree)
                    .map_err(|e| anyhow::anyhow!("rasterize error: {e}"))?;

                let page_label = pages.clone().unwrap_or_else(|| "1".to_string());
                let out_path = output.join(format!("{stem}-{page_label}.png"));
                canvas
                    .save_png(&out_path)
                    .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", out_path.display()))?;

                println!(
                    "rendered {} -> {} ({}x{}, dpi={}, parallel={})",
                    input.display(),
                    out_path.display(),
                    width,
                    height,
                    dpi,
                    parallel
                );
                Ok(())
            }
        }
        Command::Version => {
            println!("tpt-glyph {}", env!("CARGO_PKG_VERSION"));
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
