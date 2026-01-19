//! Adaptive Multi-Table (AMT) graph representation for high-performance graph processing.
//!
//! This module is vertically split:
//! - `representation`: per-node adaptive storage strategy
//! - `iter`: neighbor iteration
//! - `tests`: module-local tests

use core::sync::atomic::Ordering;

use crate::collections::ChunkedVec;
use crate::graph::access::visited::VisitedSet;

mod iter;
mod representation;
#[cfg(test)]
mod tests;

pub use iter::AmtNeighborIter;

use representation::{DENSE_THRESHOLD, SPARSE_THRESHOLD};

/// Adaptive Multi-Table graph with automatic representation selection.
#[repr(C)]
pub struct GhostAmtGraph<'brand, const EDGE_CHUNK: usize> {
    /// Node representations - adaptively chosen per node.
    pub(super) nodes: Vec<representation::NodeRepresentation>,
    /// Branded visited set for traversals (bitset-backed).
    visited: VisitedSet<'brand>,
    /// Total number of nodes.
    node_count: usize,
    /// Total number of edges.
    edge_count: usize,
    /// Chunked vector for bulk edge storage (fallback / benchmarking).
    edge_storage: ChunkedVec<usize, EDGE_CHUNK>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostAmtGraph<'brand, EDGE_CHUNK> {
    /// Creates an AMT graph with the specified number of nodes.
    ///
    /// Initially all nodes use sparse representation. Representations adapt
    /// automatically as edges are added.
    pub fn new(node_count: usize) -> Self {
        let nodes = (0..node_count)
            .map(|_| representation::NodeRepresentation::Sparse {
                neighbors: Vec::new(),
            })
            .collect();

        Self {
            nodes,
            visited: VisitedSet::new(node_count),
            node_count,
            edge_count: 0,
            edge_storage: ChunkedVec::new(),
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
    #[inline]
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count, "node index out of bounds");
        self.nodes[node].degree()
    }

    /// Checks if an edge exists between two nodes.
    #[inline]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        assert!(
            from < self.node_count && to < self.node_count,
            "node indices out of bounds"
        );
        self.nodes[from].has_edge(to)
    }

    /// Returns an iterator over the neighbors of a node.
    #[inline]
    pub fn neighbors(&self, node: usize) -> AmtNeighborIter<'_> {
        assert!(node < self.node_count, "node index out of bounds");
        self.nodes[node].neighbors(self.node_count)
    }

    /// Adds an edge to the graph, adapting representation if necessary.
    pub fn add_edge(&mut self, from: usize, to: usize) {
        assert!(
            from < self.node_count && to < self.node_count,
            "node indices out of bounds"
        );
        assert!(from != to, "self-loops not supported");

        // Don't add duplicate edges.
        if self.has_edge(from, to) {
            return;
        }

        match &mut self.nodes[from] {
            representation::NodeRepresentation::Sparse { neighbors } => {
                neighbors.push(to);
                self.edge_count += 1;
                if neighbors.len() >= SPARSE_THRESHOLD {
                    self.upgrade_representation(from);
                }
            }
            representation::NodeRepresentation::Sorted { neighbors } => {
                let insert_pos = neighbors.partition_point(|&x| x < to);
                neighbors.insert(insert_pos, to);
                self.edge_count += 1;
                if neighbors.len() >= DENSE_THRESHOLD {
                    self.upgrade_to_dense(from);
                }
            }
            representation::NodeRepresentation::Dense { bitset, degree } => {
                let word_idx = to / 64;
                let bit_idx = to % 64;
                while bitset.len() <= word_idx {
                    bitset.push(0);
                }
                if (bitset[word_idx] & (1u64 << bit_idx)) == 0 {
                    bitset[word_idx] |= 1u64 << bit_idx;
                    *degree += 1;
                    self.edge_count += 1;
                }
            }
        }
    }

    fn upgrade_representation(&mut self, node: usize) {
        let current_neighbors = match &self.nodes[node] {
            representation::NodeRepresentation::Sparse { neighbors } => neighbors.clone(),
            _ => return,
        };

        let degree = current_neighbors.len();
        if degree >= DENSE_THRESHOLD {
            self.upgrade_to_dense_with_neighbors(node, current_neighbors);
        } else {
            let mut sorted_neighbors = current_neighbors;
            sorted_neighbors.sort_unstable();
            sorted_neighbors.dedup();
            self.nodes[node] = representation::NodeRepresentation::Sorted {
                neighbors: sorted_neighbors,
            };
        }
    }

    fn upgrade_to_dense(&mut self, node: usize) {
        let current_neighbors = match &self.nodes[node] {
            representation::NodeRepresentation::Sorted { neighbors } => neighbors.clone(),
            representation::NodeRepresentation::Sparse { neighbors } => neighbors.clone(),
            _ => return,
        };
        self.upgrade_to_dense_with_neighbors(node, current_neighbors);
    }

    fn upgrade_to_dense_with_neighbors(&mut self, node: usize, neighbors: Vec<usize>) {
        let mut bitset = vec![0u64; (self.node_count + 63) / 64];
        let mut degree = 0usize;
        for &neighbor in &neighbors {
            let word_idx = neighbor / 64;
            let bit_idx = neighbor % 64;
            if (bitset[word_idx] & (1u64 << bit_idx)) == 0 {
                bitset[word_idx] |= 1u64 << bit_idx;
                degree += 1;
            }
        }
        self.nodes[node] = representation::NodeRepresentation::Dense { bitset, degree };
    }

    /// Clears the visited set for traversals.
    pub fn clear_visited(&self) {
        self.visited.clear();
    }

    /// Attempts to visit a node atomically.
    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        assert!(node < self.node_count, "node index out of bounds");
        self.visited.try_visit(node, Ordering::AcqRel)
    }
}
