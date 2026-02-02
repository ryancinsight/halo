//! A compact CSC (compressed sparse column) graph with branded, lock-free visited flags.
//!
//! CSC is the column-major equivalent of CSR, storing edges in column-major order.
//! This provides efficient access to incoming edges and transpose operations.
//!
//! Memory layout:
//! - `col_offsets`: `Vec<usize>` of length `n + 1` (column offsets)
//! - `row_indices`: chunked contiguous `usize` row indices for each column
//! - `visited`: `VisitedSet<'brand>` for lock-free concurrent traversals

use crate::collections::ChunkedVec;
use crate::graph::access::visited::VisitedSet;

/// A CSC graph whose visited bitmap is branded.
///
/// CSC (Compressed Sparse Column) stores edges in column-major order,
/// making it efficient for transpose operations and algorithms that need
/// fast access to incoming edges.
///
/// The branding is *not* required for atomic correctness; it is used to keep this
/// graph inside the Ghost branded ecosystem and prevent accidental mixing of state
/// across unrelated token scopes in larger designs.
///
/// ### Performance Characteristics
/// | Operation | Complexity | Notes |
/// |-----------|------------|-------|
/// | `from_adjacency` | \(O(n + m)\) | Builds CSC (transpose) from adjacency list |
/// | `in_neighbors` | \(O(1)\) | Returns iterator over incoming neighbors |
/// | `in_degree` | \(O(1)\) | Returns in-degree |
/// | `has_edge` | \(O(\text{in-degree})\) | Linear scan of in-neighbors |
/// | `to_csr` | \(O(n + m)\) | Converts back to CSR |
pub struct GhostCscGraph<'brand, const EDGE_CHUNK: usize> {
    col_offsets: Vec<usize>,
    row_indices: ChunkedVec<usize, EDGE_CHUNK>,
    visited: VisitedSet<'brand>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostCscGraph<'brand, EDGE_CHUNK> {
    /// Builds a CSC graph from an adjacency list.
    ///
    /// The input adjacency list represents edges from each node to its neighbors.
    /// This constructs the transpose (CSC representation).
    ///
    /// # Panics
    ///
    /// Panics if any edge references a node index out of bounds.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();

        // Count incoming edges for each node.
        let mut in_degrees = vec![0usize; n];
        for (u, neighbors) in adjacency.iter().enumerate() {
            for &v in neighbors {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                in_degrees[v] += 1;
            }
        }

        // Build column offsets (prefix sums of in-degrees).
        let mut col_offsets = Vec::with_capacity(n + 1);
        col_offsets.push(0);
        for &deg in &in_degrees {
            let last = *col_offsets.last().unwrap();
            col_offsets.push(last + deg);
        }

        // Fill row indices by position (stable: by increasing `u` scan order).
        let m = *col_offsets.last().unwrap();
        let mut rows = vec![0usize; m];
        let mut write_pos = col_offsets[..n].to_vec();
        for (u, neighbors) in adjacency.iter().enumerate() {
            for &v in neighbors {
                let idx = write_pos[v];
                rows[idx] = u;
                write_pos[v] += 1;
            }
        }

        let mut row_indices: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        row_indices.reserve(m);
        for u in rows {
            row_indices.push(u);
        }

        let visited = VisitedSet::new(n);

        Self {
            col_offsets,
            row_indices,
            visited,
        }
    }

    /// Builds a CSC graph directly from CSC parts.
    ///
    /// # Panics
    /// - if `col_offsets.len() < 2`
    /// - if offsets are not monotone
    /// - if `col_offsets.last() != row_indices.len()`
    pub fn from_csc_parts(col_offsets: Vec<usize>, row_indices: Vec<usize>) -> Self {
        assert!(col_offsets.len() >= 2, "col_offsets must have length n+1");
        let n = col_offsets.len() - 1;
        for w in col_offsets.windows(2) {
            assert!(w[0] <= w[1], "col_offsets must be monotone");
        }
        let m = *col_offsets.last().expect("col_offsets non-empty");
        assert!(
            m == row_indices.len(),
            "col_offsets last must equal row_indices length"
        );
        for &u in &row_indices {
            assert!(u < n, "row index {u} out of bounds for n={n}");
        }

        let mut r: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        r.reserve(row_indices.len());
        for u in row_indices {
            r.push(u);
        }
        let visited = VisitedSet::new(n);
        Self {
            col_offsets,
            row_indices: r,
            visited,
        }
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.col_offsets.len() - 1
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.row_indices.len()
    }

    /// Clears the visited bitmap.
    #[inline]
    pub fn reset_visited(&self) {
        self.visited.clear();
    }

    /// Returns the incoming neighbors of a node (nodes that point to this node).
    ///
    /// This is efficient in CSC representation since incoming edges are stored contiguously.
    pub fn in_neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let start = self.col_offsets[node];
        let end = self.col_offsets[node + 1];
        (start..end).map(move |i| unsafe { *self.row_indices.get_unchecked(i) })
    }

    /// Returns the in-degree of a node.
    pub fn in_degree(&self, node: usize) -> usize {
        assert!(node < self.node_count(), "node {node} out of bounds");
        let start = self.col_offsets[node];
        let end = self.col_offsets[node + 1];
        end - start
    }

    /// Checks if an edge exists from source to target.
    ///
    /// Note: This is O(in_degree(target)) in CSC representation.
    /// For frequent membership tests, consider using a different representation.
    pub fn has_edge(&self, source: usize, target: usize) -> bool {
        assert!(source < self.node_count(), "source {source} out of bounds");
        assert!(target < self.node_count(), "target {target} out of bounds");
        self.in_neighbors(target).any(|u| u == source)
    }

    /// Returns the underlying CSC data.
    ///
    /// Useful for algorithms that need direct access to the sparse matrix format.
    pub fn csc_parts(&self) -> (&[usize], &ChunkedVec<usize, EDGE_CHUNK>) {
        (&self.col_offsets, &self.row_indices)
    }
}

#[cfg(test)]
mod tests;
mod traversal;
