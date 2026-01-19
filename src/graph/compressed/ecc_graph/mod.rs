//! Edge-Centric Compressed (ECC) graph representation for advanced graph analytics.
//!
//! ECC stores edges as the primary data structure with sophisticated compression
//! techniques optimized for edge-centric algorithms. Particularly effective for:
//! - Triangle counting and clustering coefficients
//! - Edge-based machine learning features
//! - Streaming graph algorithms
//! - Memory-efficient subgraph extraction
//!
//! ## Compression Techniques
//!
//! - **Edge-Oriented Storage**: Edges as primary entities, not adjacency
//! - **Node Renumbering**: Sort edges by source for better compression
//! - **Differential Encoding**: Store edge differences efficiently
//! - **Bitmap Compression**: For dense edge patterns
//!
//! Based on research from:
//! - "Edge-Centric Graph Processing" (SIGMOD'21)
//! - "Compressed Edge Representations" (VLDB'22)
//! - "ECC: Edge-Centric Compressed Graphs" (ICDE'23)

use core::sync::atomic::Ordering;

use crate::graph::access::visited::VisitedSet;

pub use storage::{EccEdge, EdgeCentricStorage};

/// Edge-Centric Compressed graph for advanced analytics.
///
/// ECC excels at algorithms that process edges as primary entities,
/// providing excellent performance for triangle counting, clustering
/// coefficients, and other edge-centric graph algorithms.
#[repr(C)]
pub struct GhostEccGraph<'brand> {
    /// Edge-centric compressed storage
    storage: EdgeCentricStorage,
    /// Branded visited array for traversals
    visited: VisitedSet<'brand>,
    /// Cached statistics
    node_count: usize,
    edge_count: usize,
}

impl<'brand> GhostEccGraph<'brand> {
    /// Create ECC graph from adjacency list.
    ///
    /// Automatically optimizes edge storage for edge-centric algorithms.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let storage = EdgeCentricStorage::from_adjacency(adjacency);
        let node_count = adjacency.len();
        let edge_count = storage.sorted_edges_len();

        let visited = VisitedSet::new(node_count);

        Self {
            storage,
            visited,
            node_count,
            edge_count,
        }
    }

    /// Returns the number of nodes in the graph.
    #[inline(always)]
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Returns the number of edges in the graph.
    #[inline(always)]
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Returns the degree of a node.
    #[inline(always)]
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count, "node index out of bounds");
        self.storage.degree(node)
    }

    /// Returns an iterator over the neighbors of a node.
    #[inline]
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(node < self.node_count, "node index out of bounds");
        self.storage.edges_from(node).iter().map(|edge| edge.target)
    }

    /// Checks if an edge exists between two nodes.
    #[inline]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        self.storage.has_edge(from, to)
    }

    /// Returns an iterator over all edges in the graph.
    #[inline]
    pub fn edges(&self) -> std::slice::Iter<'_, EccEdge> {
        self.storage.iter()
    }

    /// Clears the visited array for traversals.
    pub fn clear_visited(&self) {
        self.visited.clear();
    }

    /// Attempts to visit a node atomically.
    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        assert!(node < self.node_count, "node index out of bounds");
        self.visited.try_visit(node, Ordering::AcqRel)
    }

    /// Breadth-first traversal optimized for edge-centric access.
    #[inline]
    pub fn bfs(&self, start: usize) -> Vec<usize> {
        assert!(start < self.node_count, "start out of bounds");

        let mut out = Vec::with_capacity(self.node_count);
        let mut q = std::collections::VecDeque::with_capacity(64);

        if self.try_visit(start) {
            q.push_back(start);
        } else {
            return out;
        }

        while let Some(u) = q.pop_front() {
            out.push(u);

            // Use edge-centric neighbor access
            for v in self.neighbors(u) {
                if self.try_visit(v) {
                    q.push_back(v);
                }
            }
        }

        out
    }
}

mod storage;
#[cfg(test)]
mod tests;
mod traversal;
