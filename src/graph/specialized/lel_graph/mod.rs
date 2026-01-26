//! Labeled Edge List (LEL) graph representation for memory-efficient graph processing.
//!
//! Vertical split:
//! - `edges`: storage + indexing
//! - `iter`: neighbor iteration
//! - `tests`: module tests

use core::sync::atomic::Ordering;

use crate::graph::access::visited::VisitedSet;
use crate::graph::compressed::ecc_graph::EccEdge;

mod edges;
mod iter;
#[cfg(test)]
mod tests;

use edges::DeltaEncodedEdges;
pub use iter::LelNeighborIter;

/// Labeled Edge List graph with compressed edge storage.
#[repr(C)]
pub struct GhostLelGraph<'brand> {
    edges: DeltaEncodedEdges,
    degrees: Vec<usize>,
    visited: VisitedSet<'brand>,
    node_count: usize,
    edge_count: usize,
}

impl<'brand> GhostLelGraph<'brand> {
    /// Create LEL graph from adjacency list.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();
        let mut degrees = vec![0usize; n];

        // For large graphs, pre-calculating total edges prevents expensive reallocations.
        // Benchmarks show a regression for small graphs (N < 20k) due to the extra pass overhead.
        let mut all_edges = if n > 20_000 {
            let mut total_edges = 0;
            for neighbors in adjacency {
                total_edges += neighbors.len();
            }
            Vec::with_capacity(total_edges)
        } else {
            Vec::new()
        };

        for (u, neighbors) in adjacency.iter().enumerate() {
            degrees[u] = neighbors.len();
            for &v in neighbors {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                all_edges.push(EccEdge::new(u, v));
            }
        }

        let edges = DeltaEncodedEdges::from_edges(n, all_edges);
        let edge_count = edges.len();
        let visited = VisitedSet::new(n);

        Self {
            edges,
            degrees,
            visited,
            node_count: n,
            edge_count,
        }
    }

    #[inline(always)]
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    #[inline(always)]
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    #[inline(always)]
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count, "node index out of bounds");
        self.degrees[node]
    }

    #[inline]
    pub fn neighbors(&self, node: usize) -> LelNeighborIter<'_> {
        assert!(node < self.node_count, "node index out of bounds");
        LelNeighborIter::new(self.edges.edges_from(node))
    }

    #[inline]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        // Neighbor slice is sorted by target.
        self.edges
            .edges_from(from)
            .binary_search_by_key(&to, |e| e.target)
            .is_ok()
    }

    pub fn clear_visited(&self) {
        self.visited.clear();
    }

    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        assert!(node < self.node_count, "node index out of bounds");
        self.visited.try_visit(node, Ordering::AcqRel)
    }

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
            for v in self.neighbors(u) {
                if self.try_visit(v) {
                    q.push_back(v);
                }
            }
        }

        out
    }

    pub fn compression_stats(&self) -> LelCompressionStats {
        let original_size = self.degrees.iter().sum::<usize>() * core::mem::size_of::<usize>();
        let compressed_size = self.edges.sorted_edges.len() * core::mem::size_of::<EccEdge>()
            + self.edges.source_indices.len() * core::mem::size_of::<usize>()
            + self.degrees.len() * core::mem::size_of::<usize>();

        LelCompressionStats {
            original_size,
            compressed_size,
            node_count: self.node_count,
            edge_count: self.edge_count,
        }
    }
}

/// Compression statistics for LEL format.
#[derive(Debug, Clone)]
pub struct LelCompressionStats {
    pub original_size: usize,
    pub compressed_size: usize,
    pub node_count: usize,
    pub edge_count: usize,
}

impl LelCompressionStats {
    #[inline]
    pub fn compression_ratio(&self) -> f64 {
        if self.compressed_size == 0 {
            0.0
        } else {
            self.original_size as f64 / self.compressed_size as f64
        }
    }

    #[inline]
    pub fn memory_savings_percent(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            let diff = if self.compressed_size > self.original_size {
                -((self.compressed_size - self.original_size) as i64)
            } else {
                (self.original_size - self.compressed_size) as i64
            };
            (diff as f64 / self.original_size as f64) * 100.0
        }
    }
}
