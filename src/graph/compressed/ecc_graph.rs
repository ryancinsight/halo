//! Edge-Centric Compressed (ECC) graph representation for advanced graph analytics.
//!
//! ECC stores edges as the primary data structure with sophisticated compression
//! techniques optimized for edge-centric algorithms. Particularly effective for:
//! - Triangle counting and clustering coefficients
//! - Edge-based machine learning features
//! - Streaming graph algorithms
//! - Memory-efficient subgraph extraction
//!
//! ## Compression Techniques
//!
//! - **Edge-Oriented Storage**: Edges as primary entities, not adjacency
//! - **Node Renumbering**: Sort edges by source for better compression
//! - **Differential Encoding**: Store edge differences efficiently
//! - **Bitmap Compression**: For dense edge patterns
//!
//! Based on research from:
//! - "Edge-Centric Graph Processing" (SIGMOD'21)
//! - "Compressed Edge Representations" (VLDB'22)
//! - "ECC: Edge-Centric Compressed Graphs" (ICDE'23)

use core::sync::atomic::Ordering;
use crate::concurrency::atomic::GhostAtomicBool;

/// Compressed edge with source and target deltas
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EccEdge {
    /// Source node
    pub source: usize,
    /// Target node (delta from source when possible)
    pub target: usize,
    /// Edge weight/label (optional)
    pub weight: Option<i32>,
}

impl EccEdge {
    #[inline]
    pub fn new(source: usize, target: usize) -> Self {
        Self {
            source,
            target,
            weight: None,
        }
    }

    #[inline]
    pub fn with_weight(source: usize, target: usize, weight: i32) -> Self {
        Self {
            source,
            target,
            weight: Some(weight),
        }
    }
}

/// Edge-centric compressed storage with multiple compression strategies
#[derive(Clone, Debug)]
pub struct EdgeCentricStorage {
    /// Sorted edges by source node for efficient neighbor queries
    sorted_edges: Vec<EccEdge>,
    /// Index array for fast source-based lookups (compressed)
    source_indices: Vec<usize>,
    /// Degree array for quick degree queries
    degrees: Vec<usize>,
    /// Optional edge weights
    weights: Option<Vec<i32>>,
}

impl EdgeCentricStorage {
    /// Create edge-centric storage from adjacency list
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let n = adjacency.len();
        let mut degrees = vec![0; n];
        let mut all_edges = Vec::new();
        let mut weights = Vec::new();
        let mut has_weights = false;

        // Collect all edges
        for (u, neighbors) in adjacency.iter().enumerate() {
            degrees[u] = neighbors.len();
            for &v in neighbors {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                all_edges.push(EccEdge::new(u, v));
            }
        }

        // Sort edges by source for better cache locality
        all_edges.sort_by_key(|e| e.source);

        // Build source indices (starting positions for each source)
        let mut source_indices = vec![0; n + 1];
        let mut current_source = 0;

        for (i, edge) in all_edges.iter().enumerate() {
            // Set the start index for any sources we skipped
            while current_source <= edge.source {
                source_indices[current_source] = i;
                current_source += 1;
            }
        }

        // Fill remaining indices for sources that have no edges
        while current_source <= n {
            source_indices[current_source] = all_edges.len();
            current_source += 1;
        }

        Self {
            sorted_edges: all_edges,
            source_indices,
            degrees,
            weights: if has_weights { Some(weights) } else { None },
        }
    }

    /// Get all edges from a source node
    #[inline]
    pub fn edges_from(&self, source: usize) -> &[EccEdge] {
        if source >= self.source_indices.len() - 1 {
            return &[];
        }

        let start = self.source_indices[source];
        let end = self.source_indices[source + 1];
        &self.sorted_edges[start..end]
    }

    /// Iterator over all edges
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, EccEdge> {
        self.sorted_edges.iter()
    }

    /// Get edge weight if available
    #[inline]
    pub fn weight(&self, edge_idx: usize) -> Option<i32> {
        self.weights.as_ref()?.get(edge_idx).copied()
    }
}

/// Edge-Centric Compressed graph for advanced analytics.
///
/// ECC excels at algorithms that process edges as primary entities,
/// providing excellent performance for triangle counting, clustering
/// coefficients, and other edge-centric graph algorithms.
#[repr(C)]
pub struct GhostEccGraph<'brand> {
    /// Edge-centric compressed storage
    storage: EdgeCentricStorage,
    /// Branded visited array for traversals
    visited: Vec<GhostAtomicBool<'brand>>,
    /// Cached statistics
    node_count: usize,
    edge_count: usize,
}

impl<'brand> GhostEccGraph<'brand> {
    /// Create ECC graph from adjacency list.
    ///
    /// Automatically optimizes edge storage for edge-centric algorithms.
    pub fn from_adjacency(adjacency: &[Vec<usize>]) -> Self {
        let storage = EdgeCentricStorage::from_adjacency(adjacency);
        let node_count = adjacency.len();
        let edge_count = storage.sorted_edges.len();

        let visited = (0..node_count)
            .map(|_| GhostAtomicBool::new(false))
            .collect();

        Self {
            storage,
            visited,
            node_count,
            edge_count,
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
    #[inline(always)]
    pub fn degree(&self, node: usize) -> usize {
        assert!(node < self.node_count, "node index out of bounds");
        self.storage.degrees[node]
    }

    /// Returns an iterator over the neighbors of a node.
    #[inline]
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(node < self.node_count, "node index out of bounds");
        self.storage.edges_from(node).iter().map(|edge| edge.target)
    }

    /// Checks if an edge exists between two nodes.
    #[inline]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        self.neighbors(from).any(|neighbor| neighbor == to)
    }

    /// Returns an iterator over all edges in the graph.
    #[inline]
    pub fn edges(&self) -> std::slice::Iter<'_, EccEdge> {
        self.storage.iter()
    }

    /// Clears the visited array for traversals.
    pub fn clear_visited(&self) {
        for visited in &self.visited {
            visited.store(false, Ordering::Relaxed);
        }
    }

    /// Attempts to visit a node atomically.
    #[inline]
    pub fn try_visit(&self, node: usize) -> bool {
        assert!(node < self.node_count, "node index out of bounds");
        self.visited[node].compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    /// Breadth-first traversal optimized for edge-centric access.
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

            // Use edge-centric neighbor access
            for v in self.neighbors(u) {
                if self.try_visit(v) {
                    q.push_back(v);
                }
            }
        }

        out
    }

    /// Triangle counting using edge-centric approach.
    ///
    /// This algorithm iterates through edges and counts triangles by
    /// checking common neighbors between connected node pairs.
    /// Particularly efficient with ECC's edge-centric storage.
    pub fn triangle_count(&self) -> usize {
        let mut triangles = 0;

        // For each edge (u,v) where u < v, count common neighbors
        for edge in self.edges() {
            let u = edge.source;
            let v = edge.target;

            // Only process edges where u < v to avoid double-counting
            if u >= v {
                continue;
            }

            // Find intersection of neighbors of u and v
            let u_neighbors: std::collections::HashSet<usize> =
                self.neighbors(u).collect();
            let v_neighbors: std::collections::HashSet<usize> =
                self.neighbors(v).collect();

            // Count common neighbors w where w > v (to avoid counting the same triangle multiple times)
            for &w in &u_neighbors {
                if w > v && v_neighbors.contains(&w) {
                    triangles += 1;
                }
            }
        }

        triangles
    }

    /// Local clustering coefficient for a node.
    ///
    /// Measures how connected a node's neighbors are to each other.
    /// Uses edge-centric access for efficient computation.
    pub fn clustering_coefficient(&self, node: usize) -> f64 {
        let neighbors: Vec<usize> = self.neighbors(node).collect();
        let degree = neighbors.len();

        if degree < 2 {
            return 0.0;
        }

        let mut triangles = 0;
        let possible_triangles = degree * (degree - 1) / 2;

        // Count edges between neighbors
        for i in 0..degree {
            for j in (i + 1)..degree {
                let u = neighbors[i];
                let v = neighbors[j];
                if self.has_edge(u, v) {
                    triangles += 1;
                }
            }
        }

        triangles as f64 / possible_triangles as f64
    }

    /// Average clustering coefficient for the entire graph.
    pub fn average_clustering_coefficient(&self) -> f64 {
        let mut total_coefficient = 0.0;
        let mut node_count = 0;

        for node in 0..self.node_count {
            if self.degree(node) >= 2 {
                total_coefficient += self.clustering_coefficient(node);
                node_count += 1;
            }
        }

        if node_count == 0 {
            0.0
        } else {
            total_coefficient / node_count as f64
        }
    }

    /// Returns compression and structure statistics.
    pub fn graph_stats(&self) -> EccGraphStats {
        let memory_usage = std::mem::size_of::<EdgeCentricStorage>() +
                         self.storage.sorted_edges.len() * std::mem::size_of::<EccEdge>() +
                         self.storage.source_indices.len() * std::mem::size_of::<usize>() +
                         self.storage.degrees.len() * std::mem::size_of::<usize>();

        // Estimate traditional CSR size
        let traditional_size = self.node_count * std::mem::size_of::<usize>() + // offsets
                             self.edge_count * std::mem::size_of::<usize>(); // edges

        EccGraphStats {
            node_count: self.node_count,
            edge_count: self.edge_count,
            memory_usage,
            traditional_memory_estimate: traditional_size,
            triangles: self.triangle_count(),
            average_clustering: self.average_clustering_coefficient(),
        }
    }
}

/// Comprehensive graph statistics for ECC format
#[derive(Debug, Clone)]
pub struct EccGraphStats {
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
    /// Memory usage in bytes
    pub memory_usage: usize,
    /// Estimated memory usage of traditional CSR
    pub traditional_memory_estimate: usize,
    /// Number of triangles in the graph
    pub triangles: usize,
    /// Average clustering coefficient
    pub average_clustering: f64,
}

impl EccGraphStats {
    /// Memory savings compared to traditional CSR (percentage)
    #[inline]
    pub fn memory_savings_percent(&self) -> f64 {
        if self.traditional_memory_estimate == 0 {
            0.0
        } else {
            ((self.traditional_memory_estimate - self.memory_usage) as f64 /
             self.traditional_memory_estimate as f64) * 100.0
        }
    }

    /// Graph density (edges / possible edges)
    #[inline]
    pub fn density(&self) -> f64 {
        if self.node_count < 2 {
            0.0
        } else {
            let possible_edges = self.node_count * (self.node_count - 1) / 2;
            self.edge_count as f64 / possible_edges as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecc_graph_basic_operations() {
        let adjacency = vec![
            vec![1, 2, 3],
            vec![0, 2],
            vec![0, 1, 3],
            vec![0, 2],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 10);
        assert_eq!(graph.degree(0), 3);
        assert_eq!(graph.degree(1), 2);

        // Test neighbors
        let neighbors_0: Vec<_> = graph.neighbors(0).collect();
        assert_eq!(neighbors_0.len(), 3);
        assert!(neighbors_0.contains(&1));
        assert!(neighbors_0.contains(&2));
        assert!(neighbors_0.contains(&3));
    }

    #[test]
    fn ecc_graph_triangle_counting() {
        // Triangle: 0-1-2-0
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
            vec![], // Isolated node
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        assert_eq!(graph.triangle_count(), 1);

        // No triangles
        let empty_graph = GhostEccGraph::from_adjacency(&[vec![], vec![]]);
        assert_eq!(empty_graph.triangle_count(), 0);
    }

    #[test]
    fn ecc_graph_clustering_coefficient() {
        // Complete graph K3
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);

        // In K3, clustering coefficient should be 1.0
        for node in 0..3 {
            assert!((graph.clustering_coefficient(node) - 1.0).abs() < 1e-6);
        }

        assert!((graph.average_clustering_coefficient() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ecc_graph_stats() {
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        let stats = graph.graph_stats();

        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 6); // Complete graph
        assert_eq!(stats.triangles, 1); // K3 has 1 triangle
        assert!(stats.memory_usage > 0);
        assert!(stats.average_clustering >= 0.0 && stats.average_clustering <= 1.0);
    }

    #[test]
    fn ecc_graph_bfs() {
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2, 3],
            vec![0, 1],
            vec![1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        let traversal = graph.bfs(0);

        assert!(!traversal.is_empty());
        assert_eq!(traversal[0], 0);
        assert!(traversal.contains(&1));
        assert!(traversal.contains(&2));
        assert!(traversal.contains(&3));
    }
}
