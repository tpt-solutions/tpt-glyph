// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-kg / validate
//
// Build an interpreter operator dispatch table directly from the knowledge
// graph, and cross-check it against the graph's `implemented` flags. This is the
// automated consistency check between the KG and the interpreter.

use crate::{KnowledgeGraph, OperatorCategory};

/// A dispatch entry derived from the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchEntry {
    pub name: String,
    pub category: OperatorCategory,
    pub implemented: bool,
}

/// Derive the operator dispatch table from the graph, in catalog order where
/// possible (operators first, then sorted by id for stability).
pub fn dispatch_table(graph: &KnowledgeGraph) -> Vec<DispatchEntry> {
    let mut entries: Vec<DispatchEntry> = graph
        .operator_nodes()
        .map(|n| {
            // Recover the category from the operator's edges is not stored on the
            // node; we keep category authoritative via the catalog path, but for a
            // graph-only build we default to GraphicsStateOp. Callers that need the
            // real category should use `dispatch_table_from_catalog`.
            DispatchEntry {
                name: n.id.clone(),
                category: OperatorCategory::GraphicsStateOp,
                implemented: n.implemented,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Build a dispatch table directly from the embedded catalog (authoritative
/// categories), suitable for feeding back into `KnowledgeGraph::validate_coverage`.
pub fn dispatch_table_from_catalog() -> Vec<DispatchEntry> {
    crate::catalog::catalog()
        .into_iter()
        .map(|d| DispatchEntry {
            name: d.name,
            category: d.category,
            implemented: d.implemented,
        })
        .collect()
}

/// The set of operator names that are actually implemented, as a `Vec<&str>` for
/// feeding `KnowledgeGraph::validate_coverage`.
pub fn implemented_names(table: &[DispatchEntry]) -> Vec<&str> {
    table
        .iter()
        .filter(|e| e.implemented)
        .map(|e| e.name.as_str())
        .collect()
}
