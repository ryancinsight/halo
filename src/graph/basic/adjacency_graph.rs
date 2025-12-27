//! A dynamic adjacency-list directed graph.
//!
//! This representation prioritizes **dynamic updates** (edge/vertex insertion and deletion)
//! while preserving halo's **ghost-token** aliasing discipline:
//! - adjacency lists are stored as `GhostCell<'brand, Vec<usize>>`
//! - reading requires `&GhostToken<'brand>`
//! - mutation requires `&mut GhostToken<'brand>`
//!
//! Visited state for concurrent traversals is stored separately in branded atomics.

use core::sync::atomic::Ordering;

use crate::{
    concurrency::atomic::GhostAtomicBool,
    concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack},
    collections::vec::BrandedVec,
    GhostToken,
};

/// A dynamic adjacency list graph whose edges are branded.
///
/// Allows efficient insertion/deletion of edges and vertices at runtime.
/// Each adjacency list is independently mutable through ghost tokens.
///
/// ### Performance Characteristics
/// | Operation | Complexity | Notes |
/// |-----------|------------|-------|
/// | `add_vertex` | \(O(1)\) amortized | Appends to internal vectors |
/// | `remove_vertex` | \(O(n + m)\) | Must scan all adjacency lists |
/// | `add_edge` | \(O(\text{out-degree})\) | Checks for existence first |
/// | `remove_edge` | \(O(\text{out-degree})\) | Linear scan of adjacency list |
/// | `out_degree` | \(O(1)\) | returns `Vec::len` |
/// | `in_degree` | \(O(n + m)\) | Scans all adjacency lists |
pub struct GhostAdjacencyGraph<'brand> {
    adjacency: BrandedVec<'brand, Vec<usize>>,
    visited: Vec<GhostAtomicBool<'brand>>,
}

impl<'brand> GhostAdjacencyGraph<'brand> {
    /// Creates an empty graph with `vertex_count` vertices and zero edges.
    pub fn new(vertex_count: usize) -> Self {
        let mut adjacency = BrandedVec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            adjacency.push(Vec::new());
        }
        let visited = (0..vertex_count)
            .map(|_| GhostAtomicBool::new(false))
            .collect();

        Self { adjacency, visited }
    }

    /// Creates a graph from adjacency lists.
    ///
    /// # Panics
    /// Panics if any neighbor index is out of bounds.
    pub fn from_adjacency(adjacency_lists: Vec<Vec<usize>>) -> Self {
        let vertex_count = adjacency_lists.len();
        for (u, nbrs) in adjacency_lists.iter().enumerate() {
            for &v in nbrs {
                assert!(v < vertex_count, "edge {u}->{v} out of bounds for n={vertex_count}");
            }
        }
        let mut adjacency = BrandedVec::with_capacity(vertex_count);
        for list in adjacency_lists {
            adjacency.push(list);
        }
        let visited = (0..vertex_count)
            .map(|_| GhostAtomicBool::new(false))
            .collect();

        Self { adjacency, visited }
    }

    /// Adds a vertex to the graph.
    ///
    /// Returns the index of the new vertex.
    pub fn add_vertex(&mut self) -> usize {
        let idx = self.adjacency.len();
        self.adjacency.push(Vec::new());
        self.visited.push(GhostAtomicBool::new(false));
        idx
    }

    /// Removes a vertex and all its incident edges.
    ///
    /// This is O(n + m) where n is vertex count and m is edge count,
    /// as it needs to remove the vertex from all adjacency lists.
    pub fn remove_vertex(&mut self, token: &mut GhostToken<'brand>, vertex: usize) {
        assert!(vertex < self.adjacency.len(), "vertex {vertex} out of bounds");

        // Remove incoming edges (u -> vertex), and shift indices above `vertex` down by 1.
        for u in 0..self.adjacency.len() {
            if u == vertex {
                continue;
            }
            let nbrs = self.adjacency.borrow_mut(token, u);
            // Remove all occurrences of `vertex`.
            nbrs.retain(|&v| v != vertex);
            // Shift indices above removed vertex.
            for v in nbrs.iter_mut() {
                if *v > vertex {
                    *v -= 1;
                }
            }
        }

        // Remove the vertex itself (outgoing edges are dropped here).
        self.adjacency.remove(vertex);
        self.visited.remove(vertex);
    }

    /// Adds a directed edge `from -> to` if it is not already present.
    ///
    /// # Panics
    /// Panics if `from` or `to` are out of bounds.
    pub fn add_edge(&self, token: &mut GhostToken<'brand>, from: usize, to: usize) {
        assert!(from < self.vertex_count(), "from vertex {from} out of bounds");
        assert!(to < self.vertex_count(), "to vertex {to} out of bounds");
        let nbrs = self.adjacency.borrow_mut(token, from);
        if !nbrs.iter().any(|&v| v == to) {
            nbrs.push(to);
        }
    }

    /// Removes a directed edge `from -> to` if present.
    ///
    /// # Panics
    /// Panics if `from` or `to` are out of bounds.
    pub fn remove_edge(&self, token: &mut GhostToken<'brand>, from: usize, to: usize) -> bool {
        assert!(from < self.vertex_count(), "from vertex {from} out of bounds");
        assert!(to < self.vertex_count(), "to vertex {to} out of bounds");
        let nbrs = self.adjacency.borrow_mut(token, from);
        let before = nbrs.len();
        nbrs.retain(|&v| v != to);
        before != nbrs.len()
    }

    /// Returns the number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.adjacency.len()
    }

    /// Returns the number of edges.
    pub fn edge_count(&self, token: &GhostToken<'brand>) -> usize {
        (0..self.vertex_count())
            .map(|u| self.out_degree(token, u))
            .sum()
    }

    /// Returns the out-degree of a vertex.
    pub fn out_degree(&self, token: &GhostToken<'brand>, vertex: usize) -> usize {
        assert!(vertex < self.vertex_count(), "vertex {vertex} out of bounds");
        self.adjacency.borrow(token, vertex).len()
    }

    /// Returns the in-degree of a vertex.
    pub fn in_degree(&self, token: &GhostToken<'brand>, vertex: usize) -> usize {
        assert!(vertex < self.vertex_count(), "vertex {vertex} out of bounds");
        let mut deg = 0usize;
        for u in 0..self.vertex_count() {
            if self.out_neighbors(token, u).any(|v| v == vertex) {
                deg += 1;
            }
        }
        deg
    }

    /// Checks if an edge exists from `from` to `to`.
    pub fn has_edge(&self, token: &GhostToken<'brand>, from: usize, to: usize) -> bool {
        assert!(from < self.vertex_count(), "from vertex {from} out of bounds");
        assert!(to < self.vertex_count(), "to vertex {to} out of bounds");
        self.out_neighbors(token, from).any(|v| v == to)
    }

    /// Returns the out-neighbors of a vertex.
    pub fn out_neighbors<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        vertex: usize,
    ) -> impl Iterator<Item = usize> + 'a {
        assert!(vertex < self.vertex_count(), "vertex {vertex} out of bounds");
        self.adjacency.borrow(token, vertex).iter().copied()
    }

    /// Returns the in-neighbors of a vertex.
    pub fn in_neighbors(&self, token: &GhostToken<'brand>, vertex: usize) -> Vec<usize> {
        assert!(vertex < self.vertex_count(), "vertex {vertex} out of bounds");
        let mut preds = Vec::new();
        for u in 0..self.vertex_count() {
            if self.out_neighbors(token, u).any(|v| v == vertex) {
                preds.push(u);
            }
        }
        preds
    }

    /// Clears all visited flags.
    pub fn reset_visited(&self) {
        for f in &self.visited {
            f.store(false, Ordering::Relaxed);
        }
    }

    /// Concurrent DFS traversal.
    pub fn dfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        stack: &GhostTreiberStack<'brand>,
    ) -> usize {
        assert!(start < self.adjacency.len(), "start vertex {start} out of bounds");

        self.reset_visited();
        self.visited[start].store(true, Ordering::Relaxed);
        stack.push(start);

        let mut count = 1;

        while let Some(vertex) = stack.pop() {
            for neighbor in self.out_neighbors(token, vertex) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    stack.push(neighbor);
                    count += 1;
                }
            }
        }

        count
    }

    /// Concurrent BFS traversal.
    pub fn bfs_reachable_count(
        &self,
        token: &GhostToken<'brand>,
        start: usize,
        deque: &GhostChaseLevDeque<'brand>,
    ) -> usize {
        assert!(start < self.adjacency.len(), "start vertex {start} out of bounds");

        self.reset_visited();
        self.visited[start].store(true, Ordering::Relaxed);
        assert!(deque.push_bottom(start), "deque capacity too small");

        let mut count = 1;

        while let Some(vertex) = deque.steal() {
            for neighbor in self.out_neighbors(token, vertex) {
                if !self.visited[neighbor].load(Ordering::Relaxed) {
                    self.visited[neighbor].store(true, Ordering::Relaxed);
                    assert!(deque.push_bottom(neighbor), "deque capacity too small");
                    count += 1;
                }
            }
        }

        count
    }

    /// Computes transitive closure using dynamic programming.
    ///
    /// Returns a matrix where matrix[i][j] is true if there's a path from i to j.
    pub fn transitive_closure(&self, token: &GhostToken<'brand>) -> Vec<Vec<bool>> {
        let n = self.adjacency.len();
        let mut closure = vec![vec![false; n]; n];

        // Reflexive closure.
        for i in 0..n {
            closure[i][i] = true;
        }

        // Initialize direct edges
        for i in 0..n {
            for j in self.out_neighbors(token, i) {
                closure[i][j] = true;
            }
        }

        // Floyd-Warshall style transitive closure
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    if closure[i][k] && closure[k][j] {
                        closure[i][j] = true;
                    }
                }
            }
        }

        closure
    }

    /// Computes strongly connected components using Kosaraju's algorithm.
    ///
    /// Returns a vector `comp` where `comp[v]` is the component id of vertex `v`.
    pub fn strongly_connected_components(&self, token: &GhostToken<'brand>) -> Vec<usize> {
        let n = self.vertex_count();

        // Build transpose adjacency in plain Vec<Vec<usize>> without mutating self.
        let mut transpose = vec![Vec::<usize>::new(); n];
        for u in 0..n {
            for v in self.out_neighbors(token, u) {
                transpose[v].push(u);
            }
        }

        // Iterative DFS to compute finishing order.
        self.reset_visited();
        let mut order = Vec::with_capacity(n);
        for start in 0..n {
            if self.visited[start].load(Ordering::Relaxed) {
                continue;
            }
            // stack of (node, neighbor_iterator)
            let mut stack = Vec::new();
            self.visited[start].store(true, Ordering::Relaxed);
            stack.push((start, self.out_neighbors(token, start)));

            while let Some((u, mut it)) = stack.pop() {
                if let Some(v) = it.next() {
                    // Push back the node and its updated iterator.
                    stack.push((u, it));
                    if !self.visited[v].load(Ordering::Relaxed) {
                        self.visited[v].store(true, Ordering::Relaxed);
                        stack.push((v, self.out_neighbors(token, v)));
                    }
                } else {
                    // All neighbors visited, record finishing order.
                    order.push(u);
                }
            }
        }

        // Second pass on transpose graph in reverse finishing order.
        // We reuse the `visited` array by treating "not visited" as "comp[v] == usize::MAX".
        let mut comp = vec![usize::MAX; n];
        let mut cid = 0usize;
        for &start in order.iter().rev() {
            if comp[start] != usize::MAX {
                continue;
            }
            let mut stack = vec![start];
            comp[start] = cid;
            while let Some(u) = stack.pop() {
                for &v in &transpose[u] {
                    if comp[v] == usize::MAX {
                        comp[v] = cid;
                        stack.push(v);
                    }
                }
            }
            cid += 1;
        }

        comp
    }

    /// Computes basic graph statistics.
    pub fn statistics(&self, token: &GhostToken<'brand>) -> GraphStatistics {
        let vertex_count = self.vertex_count();
        let edge_count = self.edge_count(token);

        let mut degrees: Vec<usize> = (0..vertex_count).map(|v| self.out_degree(token, v)).collect();
        degrees.sort_unstable();

        let (min_degree, max_degree) = match degrees.as_slice() {
            [] => (0, 0),
            [..] => (*degrees.first().unwrap(), *degrees.last().unwrap()),
        };
        let median_degree = if degrees.is_empty() {
            0
        } else if degrees.len() % 2 == 0 {
            let a = degrees[degrees.len() / 2 - 1];
            let b = degrees[degrees.len() / 2];
            (a + b) / 2
        } else {
            degrees[degrees.len() / 2]
        };

        GraphStatistics {
            vertex_count,
            edge_count,
            min_degree,
            max_degree,
            median_degree,
            average_degree: if vertex_count == 0 { 0.0 } else { edge_count as f64 / vertex_count as f64 },
        }
    }
}

/// Statistics about a graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatistics {
    /// Number of vertices.
    pub vertex_count: usize,
    /// Number of directed edges.
    pub edge_count: usize,
    /// Minimum out-degree over all vertices.
    pub min_degree: usize,
    /// Maximum out-degree over all vertices.
    pub max_degree: usize,
    /// Median out-degree over all vertices.
    pub median_degree: usize,
    /// Average out-degree \(= m/n\).
    pub average_degree: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn adjacency_graph_construction() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1, 2], // 0 -> 1,2
                vec![2],    // 1 -> 2
                vec![],     // 2
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);

            assert_eq!(graph.vertex_count(), 3);
            assert_eq!(graph.edge_count(&token), 3);
        });
    }

    #[test]
    fn adjacency_graph_dynamic_operations() {
        GhostToken::new(|mut token| {
            let mut graph = GhostAdjacencyGraph::new(3);
            graph.add_edge(&mut token, 0, 1);
            graph.add_edge(&mut token, 0, 2);
            graph.add_edge(&mut token, 1, 2);

            assert_eq!(graph.edge_count(&token), 3);
            assert!(graph.has_edge(&token, 0, 1));
            assert!(graph.has_edge(&token, 0, 2));
            assert!(graph.has_edge(&token, 1, 2));

            assert!(graph.remove_edge(&mut token, 0, 1));
            assert!(!graph.has_edge(&token, 0, 1));
            assert_eq!(graph.edge_count(&token), 2);

            // Remove vertex 1: edges involving 1 are removed and indices shift.
            graph.remove_vertex(&mut token, 1);
            assert_eq!(graph.vertex_count(), 2);
            // Previously edge 0->2 becomes 0->1 after shift.
            assert!(graph.has_edge(&token, 0, 1));
            assert_eq!(graph.edge_count(&token), 1);
        });
    }

    #[test]
    fn adjacency_graph_neighbors() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1, 2],
                vec![2],
                vec![],
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);

            assert_eq!(graph.out_neighbors(&token, 0).collect::<Vec<_>>(), vec![1, 2]);
            assert_eq!(graph.out_neighbors(&token, 1).collect::<Vec<_>>(), vec![2]);
            assert_eq!(graph.out_neighbors(&token, 2).collect::<Vec<_>>(), Vec::<usize>::new());

            assert_eq!(graph.in_neighbors(&token, 0), Vec::<usize>::new());
            assert_eq!(graph.in_neighbors(&token, 1), vec![0]);
            assert_eq!(graph.in_neighbors(&token, 2), vec![0, 1]);
        });
    }

    #[test]
    fn adjacency_graph_degrees() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1, 2],
                vec![2],
                vec![],
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);

            assert_eq!(graph.out_degree(&token, 0), 2);
            assert_eq!(graph.out_degree(&token, 1), 1);
            assert_eq!(graph.out_degree(&token, 2), 0);

            assert_eq!(graph.in_degree(&token, 0), 0);
            assert_eq!(graph.in_degree(&token, 1), 1);
            assert_eq!(graph.in_degree(&token, 2), 2);
        });
    }

    #[test]
    fn adjacency_graph_traversal() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1, 2],
                vec![2],
                vec![],
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);
            let stack = GhostTreiberStack::new(10);
            let deque = GhostChaseLevDeque::new(32);

            let dfs_count = graph.dfs_reachable_count(&token, 0, &stack);
            assert_eq!(dfs_count, 3);

            let bfs_count = graph.bfs_reachable_count(&token, 0, &deque);
            assert_eq!(bfs_count, 3);
        });
    }

    #[test]
    fn adjacency_graph_statistics() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1, 2, 3], // degree 3
                vec![2],       // degree 1
                vec![],        // degree 0
                vec![1, 2],    // degree 2
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);
            let stats = graph.statistics(&token);

            assert_eq!(stats.vertex_count, 4);
            assert_eq!(stats.edge_count, 6);
            assert_eq!(stats.min_degree, 0);
            assert_eq!(stats.max_degree, 3);
            assert_eq!(stats.median_degree, 1); // sorted: 0,1,2,3 -> median of 1,2 = 1.5 -> 1
            assert!((stats.average_degree - 1.5).abs() < 0.001);
        });
    }

    #[test]
    fn adjacency_graph_transitive_closure() {
        GhostToken::new(|token| {
            let adjacency = vec![
                vec![1],    // 0 -> 1
                vec![2],    // 1 -> 2
                vec![],     // 2
            ];

            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);
            let closure = graph.transitive_closure(&token);

            assert_eq!(closure.len(), 3);
            assert!(closure[0][0]); // self
            assert!(closure[0][1]); // direct
            assert!(closure[0][2]); // transitive
            assert!(closure[1][1]); // self
            assert!(closure[1][2]); // direct
            assert!(closure[2][2]); // self
        });
    }

    #[test]
    fn adjacency_graph_scc() {
        GhostToken::new(|token| {
            // Two SCCs: {0,1,2} cycle and {3} alone.
            let adjacency = vec![
                vec![1],
                vec![2],
                vec![0],
                vec![],
            ];
            let graph = GhostAdjacencyGraph::from_adjacency(adjacency);
            let comp = graph.strongly_connected_components(&token);
            assert_eq!(comp.len(), 4);
            assert_eq!(comp[0], comp[1]);
            assert_eq!(comp[1], comp[2]);
            assert!(comp[3] != comp[0]);
        });
    }
}
