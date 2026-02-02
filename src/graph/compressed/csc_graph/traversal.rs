//! CSC graph traversal algorithms.

use core::sync::atomic::Ordering;

use crate::{
    concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack},
    graph::compressed::csc_graph::GhostCscGraph,
    GhostToken,
};

impl<'brand, const EDGE_CHUNK: usize> GhostCscGraph<'brand, EDGE_CHUNK> {
    /// Concurrent DFS traversal starting from a node.
    ///
    /// Uses a work-stealing stack for load balancing across threads.
    /// Returns the number of reachable nodes.
    pub fn dfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        stack: &GhostTreiberStack<'brand>,
    ) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        // Reset visited flags
        self.reset_visited();

        // Mark start as visited
        debug_assert!(self.visited.try_visit(start, Ordering::Relaxed));
        stack.push(token, start);

        let mut count = 1;

        while let Some(node) = stack.pop(token) {
            // Visit all incoming neighbors (transpose traversal)
            for neighbor in self.in_neighbors(node) {
                if self.visited.try_visit(neighbor, Ordering::Relaxed) {
                    stack.push(token, neighbor);
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
    pub fn bfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        deque: &GhostChaseLevDeque<'brand>,
    ) -> usize {
        assert!(start < self.node_count(), "start {start} out of bounds");

        // Reset visited flags
        self.reset_visited();

        // Mark start as visited
        debug_assert!(self.visited.try_visit(start, Ordering::Relaxed));
        assert!(deque.push_bottom(token, start), "deque capacity too small");
        let steal_token = token.split_immutable().0;

        let mut count = 1;

        while let Some(node) = deque.steal(&steal_token) {
            // Visit all incoming neighbors (transpose traversal)
            for neighbor in self.in_neighbors(node) {
                if self.visited.try_visit(neighbor, Ordering::Relaxed) {
                    assert!(deque.push_bottom(token, neighbor), "deque capacity too small");
                    count += 1;
                }
            }
        }

        count
    }

    /// Parallel reachable count following **incoming** edges.
    ///
    /// This counts how many vertices can reach `start` in the original graph.
    pub fn parallel_reachable_count_incoming(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        threads: usize,
    ) -> usize {
        use core::sync::atomic::AtomicUsize;
        assert!(threads != 0, "threads must be > 0");
        assert!(start < self.node_count(), "start {start} out of bounds");

        self.reset_visited();
        let stack = GhostTreiberStack::new(self.node_count());
        debug_assert!(self.visited.try_visit(start, Ordering::Relaxed));
        stack.push(token, start);

        let count = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads {
                let token = token;
                scope.spawn(|| {
                    while let Some(u) = stack.pop(token) {
                        count.fetch_add(1, Ordering::Relaxed);
                        for p in self.in_neighbors(u) {
                            if self.visited.try_visit(p, Ordering::AcqRel) {
                                stack.push(token, p);
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
}
