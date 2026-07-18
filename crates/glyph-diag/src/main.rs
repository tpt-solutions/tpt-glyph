// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-diag
//
// AI-assisted diagnostic tooling. Consumes the knowledge graph to report
// operator coverage, validate the interpreter dispatch table for consistency,
// and (in later phases) surface Ghostscript-diff failures. The graph is exposed
// as an inspectable artifact via these subcommands.

use clap::{Parser, Subcommand};
use glyph_kg::{
    ingest,
    validate::{dispatch_table_from_catalog, implemented_names},
    KnowledgeGraph,
};

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
    /// Build the knowledge graph from the embedded operator catalog.
    Build {
        /// Optional output path for the JSON export. Prints to stdout if omitted.
        #[arg(long)]
        export: Option<std::path::PathBuf>,
    },
    /// Report operator coverage (implemented vs total).
    Coverage,
    /// List the isolated graphics-state sub-graph.
    StateGraph,
    /// Validate the interpreter dispatch table against the graph for consistency.
    Validate,
    /// Load an exported knowledge graph JSON and summarize it.
    Inspect { graph: std::path::PathBuf },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Build { export } => {
            let g = ingest::from_catalog();
            let json = g.to_json()?;
            match export {
                Some(p) => {
                    std::fs::write(&p, &json)?;
                    println!(
                        "wrote graph ({}) to {}",
                        g.operator_nodes().count(),
                        p.display()
                    );
                }
                None => println!("{json}"),
            }
            Ok(())
        }
        Command::Coverage => {
            let g = ingest::from_catalog();
            println!(
                "operators: {}, implemented: {}, coverage: {:.1}%",
                g.operator_nodes().count(),
                g.operator_nodes().filter(|n| n.implemented).count(),
                g.operator_coverage() * 100.0
            );
            for n in g.operator_nodes() {
                let mark = if n.implemented { "x" } else { " " };
                println!("  [{mark}] {} — {}", n.id, n.description);
            }
            Ok(())
        }
        Command::StateGraph => {
            let g = ingest::from_catalog();
            println!("isolated graphics-state sub-graph:");
            for n in g.graphics_state_nodes() {
                println!("  {} — {}", n.id, n.description);
            }
            Ok(())
        }
        Command::Validate => {
            let g = ingest::from_catalog();
            let table = dispatch_table_from_catalog();
            let implemented = implemented_names(&table);
            let issues = g.validate_coverage(&implemented);
            if issues.is_empty() {
                println!("OK: dispatch table is consistent with the knowledge graph.");
            } else {
                println!("INCONSISTENT ({} issue(s)):", issues.len());
                for i in &issues {
                    println!("  - {i}");
                }
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Inspect { graph } => {
            let g: KnowledgeGraph = {
                let s = std::fs::read_to_string(&graph)?;
                KnowledgeGraph::from_json(&s)?
            };
            println!(
                "graph: {} nodes, {} edges, coverage {:.1}%",
                g.nodes.len(),
                g.edges.len(),
                g.operator_coverage() * 100.0
            );
            println!("graphics-state sub-graph:");
            for n in g.graphics_state_nodes() {
                println!("  {} — {}", n.id, n.description);
            }
            Ok(())
        }
    }
}
