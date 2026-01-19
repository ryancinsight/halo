//! A bipartite graph with branded, lock-free visited flags.
//!
//! Bipartite graphs have two disjoint vertex sets (left and right) with edges
//! only between different sets. This structure is fundamental for matching,
//! flow, and assignment algorithms.
//!
//! Memory layout:
//! - `left_count`: number of left vertices
//! - `right_count`: number of right vertices
//! - `left_to_right`: CSR graph from left to right vertices
//! - `right_to_left`: CSC graph from right to left vertices (for transpose access)
//! - `visited_left/right`: separate visited flags for each partition

use core::sync::atomic::Ordering;

use crate::{
    collections::ChunkedVec, concurrency::worklist::GhostChaseLevDeque,
    graph::access::visited::VisitedSet,
};

/// A bipartite graph whose visited bitmaps are branded.
///
/// Edges only exist between left and right vertex sets, never within sets.
/// This enables efficient matching and flow algorithms with branded safety.
///
/// ### Performance Characteristics
/// | Operation | Complexity | Notes |
/// |-----------|------------|-------|
/// | `from_left_adjacency` | \(O(n + m)\) | Builds CSR and CSC representation |
/// | `left_neighbors` | \(O(1)\) | Out-neighbors of left vertices |
/// | `right_neighbors` | \(O(1)\) | In-neighbors of right vertices (transpose) |
/// | `left_degree`/`right_degree` | \(O(1)\) | Using cached offsets |
/// | `maximum_matching` | \(O(m\sqrt{n})\) | Hopcroft-Karp algorithm |
pub struct GhostBipartiteGraph<'brand, const EDGE_CHUNK: usize> {
    left_count: usize,
    right_count: usize,
    // CSR: left vertices -> right vertices
    left_to_right_offsets: Vec<usize>,
    left_to_right_edges: ChunkedVec<usize, EDGE_CHUNK>,
    // CSC: right vertices -> left vertices (transpose for efficient reverse lookup)
    right_to_left_offsets: Vec<usize>,
    right_to_left_edges: ChunkedVec<usize, EDGE_CHUNK>,
    visited_left: VisitedSet<'brand>,
    visited_right: VisitedSet<'brand>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostBipartiteGraph<'brand, EDGE_CHUNK> {
    /// Builds a bipartite graph from left-to-right adjacency lists.
    ///
    /// # Arguments
    /// - `left_adjacency`: Vec of right neighbors for each left vertex
    /// - `right_count`: Total number of right vertices
    ///
    /// # Panics
    /// Panics if any edge references an out-of-bounds right vertex.
    pub fn from_left_adjacency(left_adjacency: &[Vec<usize>], right_count: usize) -> Self {
        let left_count = left_adjacency.len();

        // Build CSR for left-to-right
        let mut left_offsets = Vec::with_capacity(left_count + 1);
        left_offsets.push(0);

        let mut total_edges = 0usize;
        for neighbors in left_adjacency {
            for &right in neighbors {
                assert!(right < right_count, "right vertex {right} out of bounds");
            }
            total_edges += neighbors.len();
            left_offsets.push(total_edges);
        }

        let mut left_edges: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        left_edges.reserve(total_edges);
        for neighbors in left_adjacency {
            for &right in neighbors {
                left_edges.push(right);
            }
        }

        // Build CSC-like storage for right-to-left (transpose).
        let mut right_in_degrees = vec![0usize; right_count];
        for neighbors in left_adjacency {
            for &right in neighbors {
                right_in_degrees[right] += 1;
            }
        }

        let mut right_offsets = Vec::with_capacity(right_count + 1);
        right_offsets.push(0);
        for &deg in &right_in_degrees {
            let last = *right_offsets.last().unwrap();
            right_offsets.push(last + deg);
        }

        // Fill by position to keep per-right adjacency stable by increasing `left`.
        let mut tmp = vec![0usize; total_edges];
        let mut write_pos = right_offsets[..right_count].to_vec();
        for (left, neighbors) in left_adjacency.iter().enumerate() {
            for &right in neighbors {
                let idx = write_pos[right];
                tmp[idx] = left;
                write_pos[right] += 1;
            }
        }

        let mut right_edges: ChunkedVec<usize, EDGE_CHUNK> = ChunkedVec::new();
        right_edges.reserve(total_edges);
        for left in tmp {
            right_edges.push(left);
        }

        let visited_left = VisitedSet::new(left_count);
        let visited_right = VisitedSet::new(right_count);

        Self {
            left_count,
            right_count,
            left_to_right_offsets: left_offsets,
            left_to_right_edges: left_edges,
            right_to_left_offsets: right_offsets,
            right_to_left_edges: right_edges,
            visited_left,
            visited_right,
        }
    }

    /// Number of left vertices.
    pub fn left_count(&self) -> usize {
        self.left_count
    }

    /// Number of right vertices.
    pub fn right_count(&self) -> usize {
        self.right_count
    }

    /// Total number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.left_count + self.right_count
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.left_to_right_edges.len()
    }

    /// Clears all visited flags.
    pub fn reset_visited(&self) {
        let _ = Ordering::Relaxed;
        self.visited_left.clear();
        self.visited_right.clear();
    }

    /// Returns the right neighbors of a left vertex.
    pub fn left_neighbors(&self, left: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(left < self.left_count, "left vertex {left} out of bounds");
        let start = self.left_to_right_offsets[left];
        let end = self.left_to_right_offsets[left + 1];
        (start..end).map(move |i| unsafe { *self.left_to_right_edges.get_unchecked(i) })
    }

    /// Returns the left neighbors of a right vertex.
    pub fn right_neighbors(&self, right: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(
            right < self.right_count,
            "right vertex {right} out of bounds"
        );
        let start = self.right_to_left_offsets[right];
        let end = self.right_to_left_offsets[right + 1];
        (start..end).map(move |i| unsafe { *self.right_to_left_edges.get_unchecked(i) })
    }

    /// Returns the degree of a left vertex.
    pub fn left_degree(&self, left: usize) -> usize {
        assert!(left < self.left_count, "left vertex {left} out of bounds");
        let start = self.left_to_right_offsets[left];
        let end = self.left_to_right_offsets[left + 1];
        end - start
    }

    /// Returns the degree of a right vertex.
    pub fn right_degree(&self, right: usize) -> usize {
        assert!(
            right < self.right_count,
            "right vertex {right} out of bounds"
        );
        let start = self.right_to_left_offsets[right];
        let end = self.right_to_left_offsets[right + 1];
        end - start
    }

    /// Checks if an edge exists from left to right vertex.
    pub fn has_edge(&self, left: usize, right: usize) -> bool {
        assert!(left < self.left_count, "left vertex {left} out of bounds");
        assert!(
            right < self.right_count,
            "right vertex {right} out of bounds"
        );
        self.left_neighbors(left).any(|r| r == right)
    }

    /// Computes maximum cardinality matching using Hopcroft-Karp algorithm.
    ///
    /// Returns a vector `mate` over the **global** vertex set:
    /// - for left vertices `u` in `[0, left_count)`, `mate[u] = Some(left_count + v)` if matched to right `v`
    /// - for right vertices `left_count + v`, `mate[left_count + v] = Some(u)` if matched
    pub fn maximum_matching(&self) -> Vec<Option<usize>> {
        use std::collections::VecDeque;

        const INF: i32 = i32::MAX / 4;

        let mut pair_u: Vec<Option<usize>> = vec![None; self.left_count];
        let mut pair_v: Vec<Option<usize>> = vec![None; self.right_count];
        let mut dist: Vec<i32> = vec![INF; self.left_count];

        fn bfs<'brand, const EDGE_CHUNK: usize>(
            g: &GhostBipartiteGraph<'brand, EDGE_CHUNK>,
            pair_u: &[Option<usize>],
            pair_v: &[Option<usize>],
            dist: &mut [i32],
            inf: i32,
        ) -> bool {
            let mut q = VecDeque::new();
            for u in 0..g.left_count {
                if pair_u[u].is_none() {
                    dist[u] = 0;
                    q.push_back(u);
                } else {
                    dist[u] = inf;
                }
            }

            let mut found_free = false;
            while let Some(u) = q.pop_front() {
                let du = dist[u];
                for v in g.left_neighbors(u) {
                    if let Some(u2) = pair_v[v] {
                        if dist[u2] == inf {
                            dist[u2] = du + 1;
                            q.push_back(u2);
                        }
                    } else {
                        found_free = true;
                    }
                }
            }
            found_free
        }

        fn dfs<'brand, const EDGE_CHUNK: usize>(
            g: &GhostBipartiteGraph<'brand, EDGE_CHUNK>,
            u: usize,
            pair_u: &mut [Option<usize>],
            pair_v: &mut [Option<usize>],
            dist: &mut [i32],
            inf: i32,
        ) -> bool {
            for v in g.left_neighbors(u) {
                match pair_v[v] {
                    None => {
                        pair_u[u] = Some(v);
                        pair_v[v] = Some(u);
                        return true;
                    }
                    Some(u2) => {
                        if dist[u2] == dist[u] + 1 && dfs(g, u2, pair_u, pair_v, dist, inf) {
                            pair_u[u] = Some(v);
                            pair_v[v] = Some(u);
                            return true;
                        }
                    }
                }
            }
            dist[u] = inf;
            false
        }

        while bfs(self, &pair_u, &pair_v, &mut dist, INF) {
            for u in 0..self.left_count {
                if pair_u[u].is_none() {
                    let _ = dfs(self, u, &mut pair_u, &mut pair_v, &mut dist, INF);
                }
            }
        }

        let mut mate = vec![None; self.vertex_count()];
        for u in 0..self.left_count {
            if let Some(v) = pair_u[u] {
                mate[u] = Some(self.left_count + v);
            }
        }
        for v in 0..self.right_count {
            if let Some(u) = pair_v[v] {
                mate[self.left_count + v] = Some(u);
            }
        }
        mate
    }

    /// Concurrent BFS traversal starting from a left vertex.
    ///
    /// Uses work-stealing for load balancing. Returns reachable vertex count.
    pub fn bfs_from_left(&self, start_left: usize, deque: &GhostChaseLevDeque<'brand>) -> usize {
        assert!(
            start_left < self.left_count,
            "left vertex {start_left} out of bounds"
        );

        self.reset_visited();
        debug_assert!(self.visited_left.try_visit(start_left, Ordering::Relaxed));
        assert!(deque.push_bottom(start_left), "deque capacity too small");

        let mut count = 1;

        while let Some(vertex) = deque.steal() {
            if vertex < self.left_count {
                // Left vertex - visit right neighbors
                for right in self.left_neighbors(vertex) {
                    if self.visited_right.try_visit(right, Ordering::Relaxed) {
                        assert!(
                            deque.push_bottom(self.left_count + right),
                            "deque capacity too small"
                        );
                        count += 1;
                    }
                }
            } else {
                // Right vertex - visit left neighbors
                let right = vertex - self.left_count;
                for left in self.right_neighbors(right) {
                    if self.visited_left.try_visit(left, Ordering::Relaxed) {
                        assert!(deque.push_bottom(left), "deque capacity too small");
                        count += 1;
                    }
                }
            }
        }

        count
    }

    /// Concurrent BFS traversal starting from a right vertex.
    pub fn bfs_from_right(&self, start_right: usize, deque: &GhostChaseLevDeque<'brand>) -> usize {
        assert!(
            start_right < self.right_count,
            "right vertex {start_right} out of bounds"
        );

        self.reset_visited();
        debug_assert!(self.visited_right.try_visit(start_right, Ordering::Relaxed));
        assert!(
            deque.push_bottom(self.left_count + start_right),
            "deque capacity too small"
        );

        let mut count = 1;

        while let Some(vertex) = deque.steal() {
            if vertex < self.left_count {
                // Left vertex - visit right neighbors
                for right in self.left_neighbors(vertex) {
                    if self.visited_right.try_visit(right, Ordering::Relaxed) {
                        assert!(
                            deque.push_bottom(self.left_count + right),
                            "deque capacity too small"
                        );
                        count += 1;
                    }
                }
            } else {
                // Right vertex - visit left neighbors
                let right = vertex - self.left_count;
                for left in self.right_neighbors(right) {
                    if self.visited_left.try_visit(left, Ordering::Relaxed) {
                        assert!(deque.push_bottom(left), "deque capacity too small");
                        count += 1;
                    }
                }
            }
        }

        count
    }

    /// Converts to a general graph representation (CSR format).
    ///
    /// Left vertices: [0..left_count)
    /// Right vertices: [left_count..left_count+right_count)
    pub fn to_csr_graph(&self) -> crate::graph::GhostCsrGraph<'brand, EDGE_CHUNK> {
        let total_vertices = self.vertex_count();
        let mut adjacency = vec![Vec::new(); total_vertices];

        // Add left-to-right edges
        for left in 0..self.left_count {
            for right in self.left_neighbors(left) {
                adjacency[left].push(self.left_count + right);
            }
        }

        // Add right-to-left edges (transpose)
        for right in 0..self.right_count {
            for left in self.right_neighbors(right) {
                adjacency[self.left_count + right].push(left);
            }
        }

        crate::graph::GhostCsrGraph::from_adjacency(&adjacency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn bipartite_graph_construction() {
        GhostToken::new(|_token| {
            // Left vertices: 0,1,2
            // Right vertices: 0,1
            // Edges: 0->0, 0->1, 1->0, 2->1
            let left_adjacency = vec![
                vec![0, 1], // left 0 -> right 0,1
                vec![0],    // left 1 -> right 0
                vec![1],    // left 2 -> right 1
            ];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);

            assert_eq!(graph.left_count(), 3);
            assert_eq!(graph.right_count(), 2);
            assert_eq!(graph.vertex_count(), 5);
            assert_eq!(graph.edge_count(), 4);
        });
    }

    #[test]
    fn bipartite_graph_neighbors() {
        GhostToken::new(|_token| {
            let left_adjacency = vec![vec![0, 1], vec![0], vec![1]];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);

            // Test left neighbors
            assert_eq!(graph.left_neighbors(0).collect::<Vec<_>>(), vec![0, 1]);
            assert_eq!(graph.left_neighbors(1).collect::<Vec<_>>(), vec![0]);
            assert_eq!(graph.left_neighbors(2).collect::<Vec<_>>(), vec![1]);

            // Test right neighbors
            assert_eq!(graph.right_neighbors(0).collect::<Vec<_>>(), vec![0, 1]); // lefts 0,1 point to right 0
            assert_eq!(graph.right_neighbors(1).collect::<Vec<_>>(), vec![0, 2]);
            // lefts 0,2 point to right 1
        });
    }

    #[test]
    fn bipartite_graph_degrees() {
        GhostToken::new(|_token| {
            let left_adjacency = vec![vec![0, 1], vec![0], vec![1]];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);

            assert_eq!(graph.left_degree(0), 2);
            assert_eq!(graph.left_degree(1), 1);
            assert_eq!(graph.left_degree(2), 1);

            assert_eq!(graph.right_degree(0), 2);
            assert_eq!(graph.right_degree(1), 2);
        });
    }

    #[test]
    fn bipartite_graph_has_edge() {
        GhostToken::new(|_token| {
            let left_adjacency = vec![vec![0, 1], vec![0], vec![1]];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);

            assert!(graph.has_edge(0, 0));
            assert!(graph.has_edge(0, 1));
            assert!(graph.has_edge(1, 0));
            assert!(graph.has_edge(2, 1));

            assert!(!graph.has_edge(1, 1)); // no edge
            assert!(!graph.has_edge(2, 0)); // no edge
        });
    }

    #[test]
    fn bipartite_graph_maximum_matching() {
        GhostToken::new(|_token| {
            // Complete bipartite graph K_{2,2}
            let left_adjacency = vec![
                vec![0, 1], // left 0 -> right 0,1
                vec![0, 1], // left 1 -> right 0,1
            ];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);
            let matching = graph.maximum_matching();

            // Should find a perfect matching
            assert_eq!(matching.len(), 4); // 2 left + 2 right
            assert!(matching[0].is_some()); // left 0 matched
            assert!(matching[1].is_some()); // left 1 matched
            assert!(matching[2].is_some()); // right 0 matched
            assert!(matching[3].is_some()); // right 1 matched
        });
    }

    #[test]
    fn bipartite_graph_bfs_traversal() {
        GhostToken::new(|_token| {
            let left_adjacency = vec![vec![0, 1], vec![0], vec![1]];

            let graph = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);
            let deque = GhostChaseLevDeque::new(32);

            // BFS from left vertex 0
            let reachable = graph.bfs_from_left(0, &deque);
            assert_eq!(reachable, 5); // All vertices reachable

            // BFS from right vertex 1
            let reachable = graph.bfs_from_right(1, &deque);
            assert_eq!(reachable, 5); // All vertices reachable
        });
    }

    #[test]
    fn bipartite_graph_to_csr() {
        GhostToken::new(|_token| {
            let left_adjacency = vec![vec![0], vec![1]];

            let bipartite = GhostBipartiteGraph::<1024>::from_left_adjacency(&left_adjacency, 2);
            let csr = bipartite.to_csr_graph();

            assert_eq!(csr.node_count(), 4); // 2 left + 2 right
            assert_eq!(csr.edge_count(), 4); // 2 edges + 2 reverse edges

            // Check edges
            assert_eq!(csr.neighbors(0).collect::<Vec<_>>(), vec![2]); // left 0 -> right 0 (offset by left_count)
            assert_eq!(csr.neighbors(1).collect::<Vec<_>>(), vec![3]); // left 1 -> right 1
            assert_eq!(csr.neighbors(2).collect::<Vec<_>>(), vec![0]); // right 0 -> left 0
            assert_eq!(csr.neighbors(3).collect::<Vec<_>>(), vec![1]); // right 1 -> left 1
        });
    }
}
