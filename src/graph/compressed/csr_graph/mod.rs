//! A compact CSR (compressed sparse row) graph with branded, lock-free visited flags.
//!
//! CSR is the standard sparse matrix format for graphs, storing edges in row-major order.
//! This provides efficient access to outgoing edges and is the most common graph format.
//!
//! Memory layout:
//! - `offsets`: `Vec<usize>` of length `n + 1` (row offsets)
//! - `edges`: chunked contiguous `usize` targets for each row
//! - `visited`: `VisitedSet<'brand>` for lock-free concurrent traversals

use core::sync::atomic::Ordering;

use crate::{
    collections::ChunkedVec,
    concurrency::atomic::GhostAtomicBitset,
    concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack},
    graph::access::visited::VisitedSet,
};

/// A CSR graph whose visited bitmap is branded.
///
/// CSR (Compressed Sparse Row) stores edges in row-major order,
/// making it efficient for outgoing edge access and most graph algorithms.
///
/// The branding is *not* required for atomic correctness; it is used to keep this
/// graph inside the Ghost branded ecosystem and prevent accidental mixing of state
/// across unrelated token scopes in larger designs.
///
/// ### Performance Characteristics
/// | Operation | Complexity | Notes |
/// |-----------|------------|-------|
/// | `from_adjacency` | \(O(n + m)\) | Builds CSR from adjacency list |
/// | `neighbors` | \(O(1)\) | Returns iterator over outgoing neighbors |
/// | `degree` | \(O(1)\) | Returns out-degree |
/// | `has_edge` | \(O(\text{out-degree})\) | Linear scan of neighbors |
/// | `in_neighbors` | \(O(m)\) | Scans all edges |
/// | `SIMD-friendly visited array` | Contiguous atomic booleans for potential vectorization |
#[repr(C)]
pub struct GhostCsrGraph<'brand, const EDGE_CHUNK: usize> {
    offsets: Vec<usize>,
    edges: ChunkedVec<usize, EDGE_CHUNK>,
    visited: VisitedSet<'brand>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostCsrGraph<'brand, EDGE_CHUNK> {
    /// Builds a CSR graph from an adjacency list.
    ///
    /// # Panics
    ///
    /// Panics if any edge references a node index out of bounds.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();

        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0);

        let mut total_edges = 0usize;
        for nbrs in adjacency {
            total_edges = total_edges.saturating_add(nbrs.len());
            offsets.push(total_edges);
        }

        let mut edges: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        edges.reserve(total_edges);

        for (u, nbrs) in adjacency.iter().enumerate() {
            for &v in nbrs {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                edges.push(v);
            }
        }

        let visited = VisitedSet::new(n);

        Self {
            offsets,
            edges,
            visited,
        }
    }

    /// Builds a CSR graph directly from CSR parts.
    ///
    /// # Panics
    /// - if `offsets.len() < 2`
    /// - if offsets are not monotone
    /// - if `offsets.last() != edges.len()`
    pub fn from_csr_parts(offsets: Vec<usize>, edges: Vec<usize>) -> Self {
        assert!(offsets.len() >= 2, "offsets must have length n+1");
        let n = offsets.len() - 1;
        for w in offsets.windows(2) {
            assert!(w[0] <= w[1], "offsets must be monotone");
        }
        let m = *offsets.last().expect("offsets non-empty");
        assert!(m == edges.len(), "offsets last must equal edges length");
        for &v in &edges {
            assert!(v < n, "edge to {v} out of bounds for n={n}");
        }

        let mut e: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        e.reserve(edges.len());
        for v in edges {
            e.push(v);
        }
        let visited = VisitedSet::new(n);
        Self {
            offsets,
            edges: e,
            visited,
        }
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        // `offsets` is length `n + 1` by construction.
        self.offsets.len().saturating_sub(1)
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Clears the visited bitmap.
    #[inline]
    pub fn reset_visited(&self) {
        self.visited.clear();
    }

    /// Returns `true` if `node` is currently marked visited.
    #[inline]
    pub fn is_visited(&self, node: usize) -> bool {
        self.visited.is_visited(node)
    }

    /// Marks `node` as visited and returns whether this call performed the first visit.
    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        self.visited.try_visit(node, Ordering::Relaxed)
    }

    /// Like `try_visit`, but without bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `node < self.node_count()`.
    #[inline(always)]
    unsafe fn try_visit_unchecked(&self, node: usize) -> bool {
        // SAFETY: caller guarantees bounds.
        unsafe { self.visited.try_visit_unchecked(node, Ordering::Relaxed) }
    }

    /// Returns the out-neighbors of `node`.
    ///
    /// This returns an iterator to avoid allocating a `Vec`.
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let start = self.offsets[node];
        let end = self.offsets[node + 1];
        (start..end).map(move |i| unsafe {
            // SAFETY: CSR construction ensures `i < edge_count()`.
            *self.edges.get_unchecked(i)
        })
    }

    /// Returns the in-neighbors of `node` (all `u` such that `u -> node`).
    ///
    /// This is \(O(m)\) (scan of all edges) for CSR.
    pub fn in_neighbors(&self, node: usize) -> Vec<usize> {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let mut preds = Vec::new();
        for u in 0..self.node_count() {
            if self.neighbors(u).any(|v| v == node) {
                preds.push(u);
            }
        }
        preds
    }

    /// Returns the out-degree of a node.
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count(), "node index out of bounds");
        let start = self.offsets[node];
        let end = self.offsets[node + 1];
        end - start
    }

    /// Returns the in-degree of a node.
    pub fn in_degree(&self, node: usize) -> usize {
        self.in_neighbors(node).len()
    }

    /// Checks if an edge exists from `from` to `to`.
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        assert!(from < self.node_count(), "from vertex {from} out of bounds");
        assert!(to < self.node_count(), "to vertex {to} out of bounds");
        self.neighbors(from).any(|v| v == to)
    }
}

#[cfg(test)]
mod tests;
mod traversal;
