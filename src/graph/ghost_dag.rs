//! A directed graph with DAG algorithms (topological order, critical path, DP).
//!
//! This type stores the graph in CSR form and provides DAG-specific algorithms.
//! It does **not** assume acyclicity on construction; instead, `topological_sort`
//! returns `None` when a cycle is present.

use crate::concurrency::worklist::{GhostChaseLevDeque, GhostTreiberStack};

/// A DAG whose visited bitmap is branded.
///
/// Provides topological ordering and DAG-specific algorithms.
/// The underlying representation is CSR for efficient traversal.
pub struct GhostDag<'brand, const EDGE_CHUNK: usize> {
    graph: crate::graph::GhostCsrGraph<'brand, EDGE_CHUNK>,
    topo_order: Option<Vec<usize>>,
}

impl<'brand, const EDGE_CHUNK: usize> GhostDag<'brand, EDGE_CHUNK> {
    /// Builds a DAG from adjacency lists.
    ///
    /// # Panics
    /// Panics if any edge references an out-of-bounds vertex.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let graph = crate::graph::GhostCsrGraph::from_adjacency(adjacency);

        Self {
            graph,
            topo_order: None,
        }
    }

    /// Computes topological ordering using Kahn's algorithm.
    ///
    /// Returns the topological order if the graph is acyclic, None if cyclic.
    /// The result is cached for subsequent calls.
    pub fn topological_sort(&mut self) -> Option<&[usize]> {
        if self.topo_order.is_some() {
            return self.topo_order.as_deref();
        }

        use std::collections::VecDeque;

        let n = self.graph.node_count();
        let mut indeg = vec![0usize; n];
        for u in 0..n {
            for v in self.graph.neighbors(u) {
                indeg[v] += 1;
            }
        }

        // Sources in increasing order for determinism.
        let mut q = VecDeque::new();
        for u in 0..n {
            if indeg[u] == 0 {
                q.push_back(u);
            }
        }

        let mut topo_order = Vec::with_capacity(n);
        while let Some(u) = q.pop_front() {
            topo_order.push(u);

            // Reduce in-degrees of neighbors
            for v in self.graph.neighbors(u) {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    q.push_back(v);
                }
            }
        }

        if topo_order.len() == n {
            self.topo_order = Some(topo_order);
            self.topo_order.as_deref()
        } else {
            None // Cycle detected
        }
    }

    /// Returns the cached topological ordering if computed.
    pub fn topo_order(&self) -> Option<&[usize]> {
        self.topo_order.as_deref()
    }

    /// Checks if the graph is acyclic by attempting topological sort.
    pub fn is_acyclic(&mut self) -> bool {
        self.topological_sort().is_some()
    }

    /// Computes longest path in DAG using dynamic programming.
    ///
    /// Returns the length of the longest path **from any source** to each node.
    pub fn longest_path_lengths(&mut self) -> Option<Vec<usize>> {
        let topo_order = self.topological_sort()?.to_vec();

        let n = self.graph.node_count();
        let mut dist = vec![0usize; n];

        for u in topo_order {
            for v in self.graph.neighbors(u) {
                dist[v] = dist[v].max(dist[u] + 1);
            }
        }

        Some(dist)
    }

    /// Computes shortest path in DAG using dynamic programming.
    ///
    /// Returns the length of the shortest path **from any source** to each node.
    pub fn shortest_path_lengths(&mut self) -> Option<Vec<usize>> {
        let topo_order = self.topological_sort()?.to_vec();

        let n = self.graph.node_count();
        let mut indeg = vec![0usize; n];
        for u in 0..n {
            for v in self.graph.neighbors(u) {
                indeg[v] += 1;
            }
        }

        let mut dist = vec![usize::MAX; n];
        for u in 0..n {
            if indeg[u] == 0 {
                dist[u] = 0;
            }
        }

        for u in topo_order {
            if dist[u] != usize::MAX {
                for v in self.graph.neighbors(u) {
                    dist[v] = dist[v].min(dist[u] + 1);
                }
            }
        }

        Some(dist)
    }

    /// Critical path analysis - finds the longest path in the DAG.
    ///
    /// Returns `(length, path)` where `length` is the number of edges on the path.
    pub fn critical_path(&mut self) -> Option<(usize, Vec<usize>)> {
        let topo = self.topological_sort()?.to_vec();
        let n = self.graph.node_count();

        let mut dist = vec![0usize; n];
        let mut pred: Vec<Option<usize>> = vec![None; n];

        for u in topo {
            for v in self.graph.neighbors(u) {
                let cand = dist[u] + 1;
                if cand > dist[v] {
                    dist[v] = cand;
                    pred[v] = Some(u);
                }
            }
        }

        let (end, &len) = dist.iter().enumerate().max_by_key(|&(_, &d)| d)?;
        let mut path = Vec::new();
        let mut cur = end;
        path.push(cur);
        while let Some(p) = pred[cur] {
            cur = p;
            path.push(cur);
        }
        path.reverse();
        Some((len, path))
    }

    /// Dynamic programming on DAG with custom function.
    ///
    /// Values are computed in topological order.
    ///
    /// The callback receives `(node, predecessors)` where `predecessors` is a slice of
    /// `(pred_index, &pred_value)` pairs (already computed).
    pub fn dp_compute<T, F>(&mut self, mut f: F) -> Option<Vec<T>>
    where
        F: FnMut(usize, &[(usize, &T)]) -> T,
    {
        let topo = self.topological_sort()?.to_vec();
        let n = self.graph.node_count();

        // Build predecessor lists once.
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for u in 0..n {
            for v in self.graph.neighbors(u) {
                preds[v].push(u);
            }
        }

        let mut values: Vec<Option<T>> = (0..n).map(|_| None).collect();
        for u in topo {
            let mut pairs: Vec<(usize, &T)> = Vec::with_capacity(preds[u].len());
            for &p in &preds[u] {
                let pv = values[p].as_ref().expect("topological order ensures predecessor computed");
                pairs.push((p, pv));
            }
            let out = f(u, &pairs);
            values[u] = Some(out);
        }

        Some(values.into_iter().map(|v| v.expect("all nodes computed")).collect())
    }

    // Delegate methods to underlying graph
    /// Number of vertices.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Out-neighbors iterator.
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.graph.neighbors(node)
    }

    /// In-neighbors iterator (allocates a `Vec` internally).
    pub fn in_neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.graph.in_neighbors(node).into_iter()
    }

    /// Out-degree.
    pub fn degree(&self, node: usize) -> usize {
        self.graph.degree(node)
    }

    /// In-degree.
    pub fn in_degree(&self, node: usize) -> usize {
        self.graph.in_degree(node)
    }

    /// Edge membership test.
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        self.graph.has_edge(from, to)
    }

    /// Clears the visited bitmap.
    pub fn reset_visited(&self) {
        self.graph.reset_visited();
    }

    /// DFS reachable count using the provided Treiber stack.
    pub fn dfs_reachable_count(&self, start: usize, stack: &GhostTreiberStack<'brand>) -> usize {
        self.graph.dfs_reachable_count(start, stack)
    }

    /// BFS reachable count using the provided Chase–Lev deque.
    pub fn bfs_reachable_count(&self, start: usize, deque: &GhostChaseLevDeque<'brand>) -> usize {
        self.graph.bfs_reachable_count(start, deque)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn dag_construction_and_basic_properties() {
        GhostToken::new(|_token| {
            // Simple chain: 0 -> 1 -> 2
            let adjacency = vec![
                vec![1],    // 0 -> 1
                vec![2],    // 1 -> 2
                vec![],     // 2
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);

            assert_eq!(dag.node_count(), 3);
            assert_eq!(dag.edge_count(), 2);
            assert!(dag.is_acyclic());
        });
    }

    #[test]
    fn dag_topological_sort() {
        GhostToken::new(|_token| {
            // Diamond shape: 0 -> 1, 0 -> 2, 1 -> 3, 2 -> 3
            let adjacency = vec![
                vec![1, 2], // 0 -> 1,2
                vec![3],    // 1 -> 3
                vec![3],    // 2 -> 3
                vec![],     // 3
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            let topo = dag.topological_sort().unwrap();

            // Verify topological order
            assert_eq!(topo.len(), 4);
            assert_eq!(topo[0], 0); // 0 must come first

            // 1 and 2 can be in either order
            let pos1 = topo.iter().position(|&x| x == 1).unwrap();
            let pos2 = topo.iter().position(|&x| x == 2).unwrap();
            let pos3 = topo.iter().position(|&x| x == 3).unwrap();

            assert!(pos1 < pos3);
            assert!(pos2 < pos3);
        });
    }

    #[test]
    fn dag_cycle_detection() {
        GhostToken::new(|_token| {
            // Cycle: 0 -> 1 -> 2 -> 0
            let adjacency = vec![
                vec![1],    // 0 -> 1
                vec![2],    // 1 -> 2
                vec![0],    // 2 -> 0 (cycle!)
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            assert!(!dag.is_acyclic());
            assert!(dag.topological_sort().is_none());
        });
    }

    #[test]
    fn dag_longest_path() {
        GhostToken::new(|_token| {
            // Chain: 0 -> 1 -> 2 -> 3
            let adjacency = vec![
                vec![1],
                vec![2],
                vec![3],
                vec![],
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            let lengths = dag.longest_path_lengths().unwrap();
            assert_eq!(lengths, vec![0, 1, 2, 3]);
        });
    }

    #[test]
    fn dag_shortest_path() {
        GhostToken::new(|_token| {
            // Chain: 0 -> 1 -> 2 -> 3
            let adjacency = vec![
                vec![1],
                vec![2],
                vec![3],
                vec![],
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            let lengths = dag.shortest_path_lengths().unwrap();
            assert_eq!(lengths, vec![0, 1, 2, 3]);
        });
    }

    #[test]
    fn dag_critical_path() {
        GhostToken::new(|_token| {
            // Diamond: 0 -> 1 -> 3, 0 -> 2 -> 3
            let adjacency = vec![
                vec![1, 2],
                vec![3],
                vec![3],
                vec![],
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            let (length, path) = dag.critical_path().unwrap();

            assert_eq!(length, 2); // 0 -> 1 -> 3 or 0 -> 2 -> 3
            assert_eq!(path.len(), 3);
            assert_eq!(path[0], 0);
            assert_eq!(path[2], 3);
        });
    }

    #[test]
    fn dag_dp_compute() {
        GhostToken::new(|_token| {
            // Tree: 0 -> 1,2; 1 -> 3,4; 2 -> 5
            let adjacency = vec![
                vec![1, 2],
                vec![3, 4],
                vec![5],
                vec![],
                vec![],
                vec![],
            ];

            let mut dag = GhostDag::<1024>::from_adjacency(&adjacency);
            // Number of paths from sources to each node.
            let paths = dag
                .dp_compute(|_node, preds| {
                    if preds.is_empty() {
                        1usize
                    } else {
                        preds.iter().map(|(_, v)| **v).sum()
                    }
                })
                .unwrap();

            assert_eq!(paths.len(), 6);
            assert_eq!(paths[0], 1);
            assert_eq!(paths[1], 1);
            assert_eq!(paths[2], 1);
            assert_eq!(paths[3], 1);
            assert_eq!(paths[4], 1);
            assert_eq!(paths[5], 1);
        });
    }

    #[test]
    fn dag_traversal() {
        GhostToken::new(|_token| {
            let adjacency = vec![
                vec![1, 2],
                vec![3],
                vec![3],
                vec![],
            ];

            let dag = GhostDag::<1024>::from_adjacency(&adjacency);
            let stack = GhostTreiberStack::new(10);
            let deque = GhostChaseLevDeque::new(32);

            // Test DFS
            let reachable = dag.dfs_reachable_count(0, &stack);
            assert_eq!(reachable, 4);

            // Test BFS
            let reachable = dag.bfs_reachable_count(0, &deque);
            assert_eq!(reachable, 4);
        });
    }
}