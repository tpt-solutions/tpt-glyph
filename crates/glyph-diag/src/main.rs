// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-diag
//
// AI-assisted diagnostic tooling. Consumes the knowledge graph to report
// operator coverage and (in later phases) surface Ghostscript-diff failures.
// Currently a skeleton exposing graph inspection.

use clap::{Parser, Subcommand};
use glyph_kg::{KnowledgeGraph, NodeKind};

#[derive(Parser)]
#[command(
    name = "glyph-diag",
    version,
    about = "TPT Glyph diagnostic tool (knowledge-graph driven)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Load a knowledge graph and report operator coverage.
    Coverage {
        /// Path to a `.json` knowledge graph export.
        graph: std::path::PathBuf,
    },
    /// List the isolated graphics-state sub-graph.
    StateGraph { graph: std::path::PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Coverage { graph } => {
            let g: KnowledgeGraph = load(&graph)?;
            println!(
                "operators: {}, coverage: {:.1}%",
                g.operator_nodes().count(),
                g.operator_coverage() * 100.0
            );
            for n in g.operator_nodes() {
                println!(
                    "  [{}] {} — {}",
                    if n.implemented { "x" } else { " " },
                    n.id,
                    n.description
                );
            }
            Ok(())
        }
        Command::StateGraph { graph } => {
            let g: KnowledgeGraph = load(&graph)?;
            println!("graphics-state sub-graph:");
            for n in g.graphics_state_nodes() {
                println!("  {} ({})", n.id, kind_label(n.kind));
            }
            Ok(())
        }
    }
}

fn load(path: &std::path::Path) -> anyhow::Result<KnowledgeGraph> {
    let s = std::fs::read_to_string(path)?;
    Ok(KnowledgeGraph::from_json(&s)?)
}

fn kind_label(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Operator => "operator",
        NodeKind::GraphicsState => "graphics-state",
        NodeKind::PixelEffect => "pixel-effect",
    }
}
