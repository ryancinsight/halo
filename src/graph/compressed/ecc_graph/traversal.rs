//! ECC graph traversal and analysis algorithms.

use super::storage::EccEdge;

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
            let diff = if self.memory_usage > self.traditional_memory_estimate {
                -(self.memory_usage as i64 - self.traditional_memory_estimate as i64)
            } else {
                (self.traditional_memory_estimate - self.memory_usage) as i64
            };
            (diff as f64 / self.traditional_memory_estimate as f64) * 100.0
        }
    }

    /// Triangle density (triangles per edge)
    #[inline]
    pub fn triangle_density(&self) -> f64 {
        if self.edge_count == 0 {
            0.0
        } else {
            self.triangles as f64 / self.edge_count as f64
        }
    }
}

impl<'brand> super::GhostEccGraph<'brand> {
    /// Triangle counting using edge-centric approach.
    ///
    /// This algorithm iterates through edges and counts triangles by
    /// checking common neighbors between connected node pairs.
    /// Particularly efficient with ECC's edge-centric storage.
    pub fn triangle_count(&self) -> usize {
        let mut triangles = 0;

        // For each edge (u,v) where u < v, count common out-neighbors w of u and v
        // with w > v (to avoid double-counting). Requires per-source neighbor lists
        // sorted by target (ensured in `EdgeCentricStorage::from_adjacency`).
        for edge in self.edges() {
            let u = edge.source;
            let v = edge.target;

            // Only process edges where u < v to avoid double-counting
            if u >= v {
                continue;
            }

            let u_edges = self.storage.edges_from(u);
            let v_edges = self.storage.edges_from(v);

            // Two-pointer intersection on sorted target lists.
            let mut i = 0usize;
            let mut j = 0usize;
            while i < u_edges.len() && j < v_edges.len() {
                let a = u_edges[i].target;
                let b = v_edges[j].target;
                if a < b {
                    i += 1;
                } else if b < a {
                    j += 1;
                } else {
                    if a > v {
                        triangles += 1;
                    }
                    i += 1;
                    j += 1;
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
        let memory_usage = std::mem::size_of::<super::storage::EdgeCentricStorage>() +
                         self.storage.sorted_edges_len() * std::mem::size_of::<EccEdge>() +
                         self.storage.source_indices_len() * std::mem::size_of::<usize>() +
                         self.storage.degrees_len() * std::mem::size_of::<usize>();

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
