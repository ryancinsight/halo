//! A compact CSC (compressed sparse column) graph with branded, lock-free visited flags.
//!
//! CSC is the column-major equivalent of CSR, storing edges in column-major order.
//! This provides efficient access to incoming edges and transpose operations.
//!
//! Memory layout:
//! - `col_offsets`: `Vec<usize>` of length `n + 1` (column offsets)
//! - `row_indices`: chunked contiguous `usize` row indices for each column
//! - `visited`: `Vec<GhostAtomicBool<'brand>>` for lock-free concurrent traversals

use core::sync::atomic::Ordering;

use crate::{
    collections::ChunkedVec,
    concurrency::atomic::GhostAtomicBool,
    concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack},
};

/// A CSC graph whose visited bitmap is branded.
///
/// CSC (Compressed Sparse Column) stores edges in column-major order,
/// making it efficient for transpose operations and algorithms that need
/// fast access to incoming edges.
///
/// The branding is *not* required for atomic correctness; it is used to keep this
/// graph inside the Ghost branded ecosystem and prevent accidental mixing of state
/// across unrelated token scopes in larger designs.
pub struct GhostCscGraph<'brand, const EDGE_CHUNK: usize> {
    col_offsets: Vec<usize>,
    row_indices: ChunkedVec<usize, EDGE_CHUNK>,
    visited: Vec<GhostAtomicBool<'brand>>,
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

        let visited = (0..n).map(|_| GhostAtomicBool::new(false)).collect();

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
        assert!(m == row_indices.len(), "col_offsets last must equal row_indices length");
        for &u in &row_indices {
            assert!(u < n, "row index {u} out of bounds for n={n}");
        }

        let mut r: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        r.reserve(row_indices.len());
        for u in row_indices {
            r.push(u);
        }
        let visited = (0..n).map(|_| GhostAtomicBool::new(false)).collect();
        Self {
            col_offsets,
            row_indices: r,
            visited,
        }
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.visited.len()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.row_indices.len()
    }

    /// Clears the visited bitmap.
    #[inline]
    pub fn reset_visited(&self) {
        for f in &self.visited {
            f.store(false, Ordering::Relaxed);
        }
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

    /// Concurrent DFS traversal starting from a node.
    ///
    /// Uses a work-stealing stack for load balancing across threads.
    /// Returns the number of reachable nodes.
    pub fn dfs_reachable_count(&self, start: usize, stack: &GhostTreiberStack<'brand>) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        // Reset visited flags
        self.reset_visited();

        // Mark start as visited
        self.visited[start].store(true, Ordering::Relaxed);
        stack.push(start);

        let mut count = 1;

        while let Some(node) = stack.pop() {
            // Visit all incoming neighbors (transpose traversal)
            for neighbor in self.in_neighbors(node) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    stack.push(neighbor);
                    count += 1;
                }
            }
        }

        count
    }

    /// Concurrent BFS traversal starting from a node.
    ///
    /// Uses a work-stealing deque for load balancing across threads.
    /// Returns the number of reachable nodes.
    pub fn bfs_reachable_count(&self, start: usize, deque: &GhostChaseLevDeque<'brand>) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        // Reset visited flags
        self.reset_visited();

        // Mark start as visited
        self.visited[start].store(true, Ordering::Relaxed);
        assert!(deque.push_bottom(start), "deque capacity too small");

        let mut count = 1;

        while let Some(node) = deque.steal() {
            // Visit all incoming neighbors (transpose traversal)
            for neighbor in self.in_neighbors(node) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    assert!(deque.push_bottom(neighbor), "deque capacity too small");
                    count += 1;
                }
            }
        }

        count
    }

    /// Parallel reachable count following **incoming** edges.
    ///
    /// This counts how many vertices can reach `start` in the original graph.
    pub fn parallel_reachable_count_incoming(&self, start: usize, threads: usize) -> usize {
        use core::sync::atomic::AtomicUsize;
        assert!(threads != 0, "threads must be > 0");
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        let stack = GhostTreiberStack::new(self.node_count());
        self.visited[start].store(true, Ordering::Relaxed);
        stack.push(start);

        let count = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    while let Some(u) = stack.pop() {
                        count.fetch_add(1, Ordering::Relaxed);
                        for p in self.in_neighbors(u) {
                            if self.visited[p]
                                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                                .is_ok()
                            {
                                stack.push(p);
                            }
                        }
                    }
                });
            }
        });

        count.load(Ordering::Relaxed)
    }

    /// Computes the transpose of this CSC graph (returns a CSR graph).
    ///
    /// Since CSC is already the transpose representation, this converts
    /// back to CSR format by reversing the transformation.
    pub fn to_csr(&self) -> crate::graph::GhostCsrGraph<'brand, EDGE_CHUNK> {
        let n = self.node_count();
        let mut adjacency = vec![Vec::new(); n];

        // For each column `v` (target), row indices are sources `u` for edges `u -> v`.
        for v in 0..n {
            let start = self.col_offsets[v];
            let end = self.col_offsets[v + 1];
            for i in start..end {
                let u = unsafe { *self.row_indices.get_unchecked(i) };
                adjacency[u].push(v);
            }
        }

        crate::graph::GhostCsrGraph::from_adjacency(&adjacency)
    }

    /// Returns the underlying CSC data.
    ///
    /// Useful for algorithms that need direct access to the sparse matrix format.
    pub fn csc_parts(&self) -> (&[usize], &ChunkedVec<usize, EDGE_CHUNK>) {
        (&self.col_offsets, &self.row_indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn csc_graph_from_adjacency() {
        GhostToken::new(|_token| {
            // Create a simple graph: 0->1, 0->2, 1->2
            let adjacency = vec![
                vec![1, 2], // node 0
                vec![2],    // node 1
                vec![],     // node 2
            ];

            let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);

            assert_eq!(csc.node_count(), 3);
            assert_eq!(csc.edge_count(), 3);

            // Check in-neighbors (transpose edges)
            assert_eq!(csc.in_neighbors(0).collect::<Vec<_>>(), Vec::<usize>::new()); // no one points to 0
            assert_eq!(csc.in_neighbors(1).collect::<Vec<_>>(), vec![0]); // 0->1 becomes 0 in column 1
            assert_eq!(csc.in_neighbors(2).collect::<Vec<_>>(), vec![0, 1]); // 0->2 and 1->2 become [0,1] in column 2
        });
    }

    #[test]
    fn csc_graph_dfs_traversal() {
        GhostToken::new(|_token| {
            // Create a graph: 0->1->2, 0->2
            let adjacency = vec![
                vec![1, 2], // node 0
                vec![2],    // node 1
                vec![],     // node 2
            ];

            let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
            let stack = GhostTreiberStack::new(10);

            // Incoming traversal: nodes that can reach `start` in the original graph.
            assert_eq!(csc.dfs_reachable_count(0, &stack), 1);
            assert_eq!(csc.dfs_reachable_count(2, &stack), 3);

        });
    }

    #[test]
    fn csc_graph_bfs_traversal() {
        GhostToken::new(|_token| {
            // Create a graph: 0->1->2, 0->2
            let adjacency = vec![
                vec![1, 2], // node 0
                vec![2],    // node 1
                vec![],     // node 2
            ];

            let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
            let deque = GhostChaseLevDeque::new(32);

            assert_eq!(csc.bfs_reachable_count(0, &deque), 1);
            assert_eq!(csc.bfs_reachable_count(2, &deque), 3);
        });
    }

    #[test]
    fn csc_graph_to_csr_conversion() {
        GhostToken::new(|_token| {
            let adjacency = vec![
                vec![1, 2],
                vec![2],
                vec![],
            ];

            let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
            let csr = csc.to_csr();

            assert_eq!(csr.node_count(), 3);
            assert_eq!(csr.edge_count(), 3);

            // Verify the conversion is correct
            assert_eq!(csr.neighbors(0).collect::<Vec<_>>(), vec![1, 2]);
            assert_eq!(csr.neighbors(1).collect::<Vec<_>>(), vec![2]);
            assert_eq!(csr.neighbors(2).collect::<Vec<_>>(), Vec::<usize>::new());
        });
    }

    #[test]
    fn csc_graph_degrees_and_membership() {
        GhostToken::new(|_token| {
            let adjacency = vec![
                vec![1, 2],
                vec![2],
                vec![],
            ];

            let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);

            assert_eq!(csc.in_degree(0), 0); // no incoming edges
            assert_eq!(csc.in_degree(1), 1); // one incoming edge (0->1)
            assert_eq!(csc.in_degree(2), 2); // two incoming edges (0->2, 1->2)

            assert!(!csc.has_edge(0, 0)); // no self-loops
            assert!(csc.has_edge(0, 1));  // 0->1 exists
            assert!(csc.has_edge(0, 2));  // 0->2 exists
            assert!(csc.has_edge(1, 2));  // 1->2 exists
            assert!(!csc.has_edge(2, 0)); // 2->0 doesn't exist
        });
    }
}