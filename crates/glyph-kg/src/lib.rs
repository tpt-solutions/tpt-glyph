// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — glyph-kg
//
// The Rendering Pipeline Knowledge Graph. This crate models how PostScript/PDF
// operators affect the graphics state and the pixel buffer. The "Graphics State"
// is explicitly isolated as a distinct sub-graph so the interpreter can be driven
// by it and the dangerous global-state variables of the legacy C implementation
// eliminated.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Category of a graph node, distinguishing the isolated graphics-state sub-graph
/// from operator nodes and pixel-buffer effect nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// An interpreter/rendering operator (e.g. `moveto`, `setrgbcolor`).
    Operator,
    /// A graphics-state attribute belonging to the isolated graphics-state sub-graph
    /// (e.g. `stroke_color`, `ctm`).
    GraphicsState,
    /// An effect on the pixel buffer (e.g. `paints_path`, `clears_canvas`).
    PixelEffect,
}

/// A node in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    /// Whether this operator is currently implemented in the interpreter.
    pub implemented: bool,
    /// Human-readable description.
    pub description: String,
}

/// A directed edge: `from` acts upon / modifies / produces `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Relationship label (e.g. "modifies", "produces", "consumes").
    pub relation: String,
}

/// The full rendering pipeline knowledge graph.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: HashMap<String, Node>,
    pub edges: Vec<Edge>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a node.
    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add a directed edge (deduplicated by content).
    pub fn add_edge(&mut self, edge: Edge) {
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
    }

    /// All graphics-state nodes (the isolated sub-graph).
    pub fn graphics_state_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .values()
            .filter(|n| n.kind == NodeKind::GraphicsState)
    }

    /// All operator nodes.
    pub fn operator_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values().filter(|n| n.kind == NodeKind::Operator)
    }

    /// Fraction of operators that are implemented (coverage in `[0,1]`).
    pub fn operator_coverage(&self) -> f64 {
        let ops: Vec<&Node> = self.operator_nodes().collect();
        if ops.is_empty() {
            return 0.0;
        }
        let done = ops.iter().filter(|n| n.implemented).count();
        done as f64 / ops.len() as f64
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_coverage_and_subgraph() {
        let mut g = KnowledgeGraph::new();
        g.add_node(Node {
            id: "setrgbcolor".into(),
            kind: NodeKind::Operator,
            implemented: true,
            description: "Set stroke/fill color.".into(),
        });
        g.add_node(Node {
            id: "stroke_color".into(),
            kind: NodeKind::GraphicsState,
            implemented: true,
            description: "Current stroke color.".into(),
        });
        g.add_edge(Edge {
            from: "setrgbcolor".into(),
            to: "stroke_color".into(),
            relation: "modifies".into(),
        });

        assert_eq!(g.operator_coverage(), 1.0);
        assert_eq!(g.graphics_state_nodes().count(), 1);
        assert_eq!(g.operator_nodes().count(), 1);

        let json = g.to_json().unwrap();
        let back = KnowledgeGraph::from_json(&json).unwrap();
        assert_eq!(g, back);
    }
}
