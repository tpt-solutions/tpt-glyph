// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-cli
//
// Command-line front-end. Currently a skeleton; rendering commands are wired in
// later phases.

use clap::{Parser, Subcommand};

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
            println!(
                "render {} -> {} (dpi={}, pages={:?})",
                input.display(),
                output.display(),
                dpi,
                pages
            );
            anyhow::bail!("rendering not yet implemented (see todo Phase 5/6)")
        }
        Command::Version => {
            println!("glyph {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
