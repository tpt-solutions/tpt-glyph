// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-kg / ingest
//
// Ingestion tooling: build a `KnowledgeGraph` from the operator catalog and/or a
// serialized definition file. The resulting graph isolates the graphics-state
// sub-graph and wires operator -> state/effect edges.

use crate::{KnowledgeGraph, OperatorDef};
use std::path::Path;

/// Build the full knowledge graph from the embedded catalog. This is the
/// canonical graph the interpreter dispatch table is validated against.
pub fn from_catalog() -> KnowledgeGraph {
    let mut g = KnowledgeGraph::with_graphics_state_subgraph();
    for def in crate::catalog::catalog() {
        g.ingest_operator(def);
    }
    g
}

/// Parse a catalog definition file (one JSON `OperatorDef` per line, or a JSON
/// array) and ingest into a fresh graph.
pub fn from_definition_file(path: &Path) -> Result<KnowledgeGraph, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let defs: Vec<OperatorDef> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text)?
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<OperatorDef>)
            .collect::<Result<_, _>>()?
    };
    let mut g = KnowledgeGraph::with_graphics_state_subgraph();
    for d in defs {
        g.ingest_operator(d);
    }
    Ok(g)
}
