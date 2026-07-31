// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-diag
//
// AI-assisted diagnostic tooling. Consumes the knowledge graph to report
// operator coverage, validate the interpreter dispatch table for consistency,
// and (in later phases) surface Ghostscript-diff failures. The graph is exposed
// as an inspectable artifact via these subcommands.

use clap::{Parser, Subcommand};
use tpt_glyph_kg::{
    ingest,
    validate::{dispatch_table_from_catalog, implemented_names},
    KnowledgeGraph,
};

#[derive(Parser)]
#[command(
    name = "tpt-glyph-diag",
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
    /// Analyze a tpt-glyph-diff report (Phase 1) and suggest likely causes.
    Diff {
        /// Path to a diff-report.json produced by tpt-glyph-diff.
        report: std::path::PathBuf,
    },
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
        Command::Diff { report } => {
            let raw = std::fs::read_to_string(&report)?;
            let parsed: serde_json::Value = serde_json::from_str(&raw)?;
            let fixtures = parsed
                .get("fixtures")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            // A fixture is "failing" when its verdict is "Fail" (pending fixtures
            // have no metrics and are not actionable here).
            let failed: Vec<&serde_json::Value> = fixtures
                .iter()
                .filter(|f| f.get("verdict").and_then(|v| v.as_str()) == Some("Fail"))
                .collect();

            println!(
                "diff analysis: {} fixture(s), {} failing",
                fixtures.len(),
                failed.len()
            );

            // Cross-reference failing fixtures with the knowledge graph so we can
            // suggest whether a miss is plausibly an unimplemented/missing operator.
            let g = ingest::from_catalog();
            let table = dispatch_table_from_catalog();
            let implemented = implemented_names(&table);
            let missing_ops: Vec<String> = g
                .operator_nodes()
                .filter(|n| !n.implemented && !implemented.contains(&n.id.as_str()))
                .map(|n| n.id.clone())
                .collect();

            for f in &failed {
                let name = f
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>");
                let note = f.get("note").and_then(|v| v.as_str()).unwrap_or("");
                println!();
                println!("✗ {name} — {note}");
                if let Some(m) = f.get("metrics") {
                    if let (Some(mse), Some(peak), Some(ssim)) = (
                        m.get("mse").and_then(|v| v.as_f64()),
                        m.get("peak_error").and_then(|v| v.as_u64()),
                        m.get("ssim").and_then(|v| v.as_f64()),
                    ) {
                        print_metrics_raw(mse, peak as u8, ssim);
                    }
                }
                print_suggestions(name, &missing_ops);
            }

            if failed.is_empty() {
                println!("All fixtures pass (or are pending). No action required.");
            }
            Ok(())
        }
    }
}

/// Print a compact metrics block for a failed fixture.
fn print_metrics_raw(mse: f64, peak_error: u8, ssim: f64) {
    println!(
        "    mse={:.4} peak_error={} ssim={:.4}",
        mse, peak_error, ssim
    );
    let mut hints = Vec::new();
    if peak_error >= 200 {
        hints.push("likely a missing/blank render or wrong page size");
    }
    if mse > 0.05 {
        hints.push("large global divergence — gross geometry/color mismatch");
    }
    if ssim < 0.9 {
        hints.push("structural dissimilarity — check transforms or fill rule");
    }
    if !hints.is_empty() {
        for h in hints {
            println!("    hint: {h}");
        }
    }
}

/// Suggest likely causes for a failing fixture, informed by the KG.
fn print_suggestions(name: &str, missing_ops: &[String]) {
    if missing_ops.is_empty() {
        println!("    likely cause: implemented operators produce divergent output; check rasterizer/clip handling");
        return;
    }
    // Heuristic: name-derived hints (e.g. "state" fixtures exercise graphics state).
    let mut related = Vec::new();
    if name.contains("state") {
        related.extend(missing_ops.iter().filter(|o| is_state_op(o)).cloned());
    }
    if name.contains("line") || name.contains("shape") {
        related.extend(missing_ops.iter().filter(|o| is_path_op(o)).cloned());
    }
    if related.is_empty() {
        related = missing_ops.to_vec();
    }
    println!(
        "    related unimplemented operators (KG): {}",
        related.join(", ")
    );
}

fn is_state_op(op: &str) -> bool {
    matches!(
        op,
        "setrgbcolor" | "setgray" | "setlinewidth" | "setlinecap" | "setlinejoin" | "concat"
    )
}

fn is_path_op(op: &str) -> bool {
    matches!(
        op,
        "moveto" | "lineto" | "curveto" | "closepath" | "stroke" | "fill" | "eofill" | "clip"
    )
}
