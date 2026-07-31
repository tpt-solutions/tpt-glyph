// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-kg
//
// The Rendering Pipeline Knowledge Graph. This crate models how PostScript/PDF
// operators affect the graphics state and the pixel buffer. The "Graphics State"
// is explicitly isolated as a distinct sub-graph so the interpreter can be driven
// by it and the dangerous global-state variables of the legacy C implementation
// eliminated.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod catalog;
pub mod ingest;
pub mod validate;

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

/// Functional category of an operator, used to group the catalog and to drive
/// which interpreter subsystem handles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorCategory {
    /// Path construction: `moveto`, `lineto`, `curveto`, `closepath`, ...`
    PathConstruction,
    /// Path painting: `stroke`, `fill`, `clip`, ...
    PathPainting,
    /// Graphics state: `gsave`, `grestore`, `setrgbcolor`, `setlinewidth`, `concat`, ...
    GraphicsStateOp,
    /// Stack / dictionary: `def`, `pop`, `dup`, ...
    Stack,
    /// Device / show: `showpage`, `show`, ...
    Device,
}

/// A declarative operator definition used by the catalog and ingestion tooling.
///
/// This is the "PostScript operator definition" the knowledge graph is built
/// from: it names the operator, its category, the graphics-state attributes it
/// touches, and any pixel-buffer effects it produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorDef {
    pub name: String,
    pub category: OperatorCategory,
    pub description: String,
    /// Graphics-state attribute ids this operator modifies/reads (the isolated
    /// sub-graph nodes it connects to).
    pub affects_state: Vec<String>,
    /// Pixel-buffer effect ids this operator produces (e.g. "paints_path").
    pub produces_effects: Vec<String>,
    /// Whether the interpreter currently implements this operator.
    pub implemented: bool,
}

impl OperatorDef {
    /// Build a `KnowledgeGraph` node + edges for this operator definition.
    pub fn into_graph_nodes(self) -> (Node, Vec<Edge>) {
        let node = Node {
            id: self.name.clone(),
            kind: NodeKind::Operator,
            implemented: self.implemented,
            description: self.description.clone(),
        };
        let mut edges = Vec::new();
        for s in &self.affects_state {
            edges.push(Edge {
                from: self.name.clone(),
                to: s.clone(),
                relation: "modifies".into(),
            });
        }
        for e in &self.produces_effects {
            edges.push(Edge {
                from: self.name.clone(),
                to: e.clone(),
                relation: "produces".into(),
            });
        }
        (node, edges)
    }
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

    /// Seed the graph with the canonical isolated **graphics-state sub-graph**:
    /// the distinct, isolated state attributes that operators may modify. This
    /// mirrors the explicit isolation of "Graphics State" in the knowledge graph
    /// strategy and replaces Ghostscript's global state variables.
    pub fn with_graphics_state_subgraph() -> Self {
        let mut g = Self::new();
        for (id, desc) in [
            ("stroke_color", "Current stroke color (RGB)."),
            ("fill_color", "Current fill color (RGB)."),
            ("line_width", "Current line width in user space."),
            ("ctm", "Current transformation matrix (user -> device)."),
            ("clip_path", "Current clipping path."),
            ("line_cap", "Current line cap style."),
            ("line_join", "Current line join style."),
        ] {
            g.add_node(Node {
                id: id.into(),
                kind: NodeKind::GraphicsState,
                implemented: true,
                description: desc.into(),
            });
        }
        g
    }

    /// Ingest a single operator definition into the graph, creating its operator
    /// node and edges to the graphics-state / pixel-effect nodes it touches.
    pub fn ingest_operator(&mut self, def: OperatorDef) {
        let (node, edges) = def.into_graph_nodes();
        self.add_node(node);
        for e in edges {
            self.add_edge(e);
        }
    }

    /// Validate that every operator node referenced by an interpreter dispatch
    /// table (`implemented_names`) is present and marked implemented in the
    /// graph, and report any graph operators lacking implementation.
    ///
    /// Returns the list of consistency issues (empty == valid).
    pub fn validate_coverage(&self, implemented_names: &[&str]) -> Vec<String> {
        let implemented: std::collections::HashSet<&str> =
            implemented_names.iter().copied().collect();
        let mut issues = Vec::new();

        for op in self.operator_nodes() {
            if op.implemented && !implemented.contains(op.id.as_str()) {
                issues.push(format!(
                    "operator '{}' marked implemented in KG but missing from dispatch table",
                    op.id
                ));
            }
            if !op.implemented && implemented.contains(op.id.as_str()) {
                issues.push(format!(
                    "operator '{}' present in dispatch table but not marked implemented in KG",
                    op.id
                ));
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest;
    use crate::validate::{dispatch_table_from_catalog, implemented_names};

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

    #[test]
    fn catalog_graph_contains_isolated_graphics_state_subgraph() {
        let g = ingest::from_catalog();
        // The isolated graphics-state sub-graph must always be present.
        let gs: Vec<&str> = g.graphics_state_nodes().map(|n| n.id.as_str()).collect();
        for attr in [
            "stroke_color",
            "fill_color",
            "line_width",
            "ctm",
            "clip_path",
        ] {
            assert!(gs.contains(&attr), "missing graphics-state node {attr}");
        }
        // Every catalog operator should be present as an operator node.
        for def in crate::catalog::catalog() {
            assert!(
                g.nodes.contains_key(&def.name),
                "missing operator {}",
                def.name
            );
        }
    }

    #[test]
    fn catalog_operator_edges_reach_graphics_state() {
        let g = ingest::from_catalog();
        // `setrgbcolor` must have an edge modifying stroke_color/fill_color.
        let edges: Vec<&Edge> = g.edges.iter().filter(|e| e.from == "setrgbcolor").collect();
        assert!(edges
            .iter()
            .any(|e| e.to == "stroke_color" && e.relation == "modifies"));
        assert!(edges
            .iter()
            .any(|e| e.to == "fill_color" && e.relation == "modifies"));
        // `stroke` must produce the paints_path pixel effect.
        let stroke_edges: Vec<&Edge> = g.edges.iter().filter(|e| e.from == "stroke").collect();
        assert!(stroke_edges
            .iter()
            .any(|e| e.to == "paints_path" && e.relation == "produces"));
    }

    #[test]
    fn dispatch_table_is_consistent_with_graph() {
        let g = ingest::from_catalog();
        let table = dispatch_table_from_catalog();
        let implemented = implemented_names(&table);
        // Catalog is the source of truth, so the graph built from it must validate
        // cleanly against the catalog-derived dispatch table.
        let issues = g.validate_coverage(&implemented);
        assert!(issues.is_empty(), "coverage issues: {issues:?}");
    }

    #[test]
    fn validate_flags_dispatch_graph_mismatch() {
        let mut g = ingest::from_catalog();
        // Mark an implemented operator (`setgray`) as unimplemented in the graph
        // only; the catalog-derived dispatch table still reports it implemented,
        // so validation must flag the mismatch.
        if let Some(n) = g.nodes.get_mut("setgray") {
            n.implemented = false;
        }
        let table = dispatch_table_from_catalog();
        let implemented = implemented_names(&table);
        let issues = g.validate_coverage(&implemented);
        assert!(
            issues.iter().any(|i| i.contains("setgray")),
            "expected setgray mismatch, got {issues:?}"
        );
    }
}
