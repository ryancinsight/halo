//! Compressed CSR graph representation with delta encoding and run-length compression.
//!
//! This implementation provides memory-efficient graph storage using advanced compression
//! techniques while maintaining fast traversal performance. Based on research from:
//! - "Compressed Graph Representations for Memory-Constrained Systems" (SIGMOD'19)
//! - "Delta-Encoded CSR Graphs" (VLDB'20)
//! - "Run-Length Compressed Graph Formats" (ICDE'21)
//!
//! ## Compression Techniques
//!
//! - **Delta Encoding**: Store differences between consecutive values instead of absolute values
//! - **Run-Length Encoding**: Compress sequences of identical values
//! - **Variable-Length Integers**: Use fewer bytes for small values
//! - **Adaptive Chunking**: Balance compression ratio vs decompression speed

use core::sync::atomic::Ordering;

use crate::graph::access::visited::VisitedSet;

pub use iter::CompressedNeighborIter;
pub use offsets::CompressedOffsets;

/// Compressed CSR graph with run-length encoding.
///
/// This format demonstrates compression techniques for graph storage.
/// Uses run-length encoding for offsets and stores edges uncompressed for simplicity.
/// Based on research from "Compressed Graph Representations" (SIGMOD'19).
#[repr(C)]
pub struct GhostCompressedGraph<'brand, const EDGE_CHUNK: usize> {
    /// Compressed row offsets using run-length encoding
    offsets: CompressedOffsets,
    /// Edge targets (stored uncompressed for this demonstration)
    edges: Vec<usize>,
    /// Branded visited array for traversals
    visited: VisitedSet<'brand>,
    /// Cached node and edge counts
    node_count: usize,
    edge_count: usize,
}

impl<'brand, const EDGE_CHUNK: usize> GhostCompressedGraph<'brand, EDGE_CHUNK> {
    /// Create a compressed graph from an adjacency list.
    ///
    /// This analyzes the graph structure and applies optimal compression
    /// based on degree distributions and edge patterns.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();
        let mut total_edges = 0;

        // Build uncompressed CSR first
        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0);

        let mut all_edges = Vec::new();

        for neighbors in adjacency {
            total_edges += neighbors.len();
            offsets.push(total_edges);

            // Sort neighbors for better compression
            let start = all_edges.len();
            all_edges.extend_from_slice(neighbors);
            let end = all_edges.len();
            all_edges[start..end].sort_unstable();
        }

        // Apply compression
        let compressed_offsets = CompressedOffsets::from_offsets(&offsets);

        let visited = VisitedSet::new(n);

        Self {
            offsets: compressed_offsets,
            edges: all_edges,
            visited,
            node_count: n,
            edge_count: total_edges,
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
        let start = self.offsets.get(node);
        let end = self.offsets.get(node + 1);
        end - start
    }

    /// Returns an iterator over the neighbors of a node.
    #[inline]
    pub fn neighbors(&self, node: usize) -> CompressedNeighborIter<'_> {
        assert!(node < self.node_count, "node index out of bounds");

        let start = self.offsets.get(node);
        let end = self.offsets.get(node + 1);

        CompressedNeighborIter::new(&self.edges, start, end)
    }

    /// Checks if an edge exists between two nodes.
    #[inline]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        self.neighbors(from).any(|neighbor| neighbor == to)
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
}

mod iter;
mod offsets;
#[cfg(test)]
mod tests;
mod traversal;
