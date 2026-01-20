//! Edge-centric compressed storage types.

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
        let weights = Vec::new();
        let has_weights = false;

        // Collect all edges
        for (u, neighbors) in adjacency.iter().enumerate() {
            degrees[u] = neighbors.len();
            for &v in neighbors {
                assert!(v < n, "edge {u}->{v} is out of bounds for n={n}");
                all_edges.push(EccEdge::new(u, v));
            }
        }

        // Sort edges by (source, target) so per-source neighbor lists are sorted by target.
        // This enables binary-search membership tests and allocation-free intersections.
        all_edges.sort_unstable_by_key(|e| (e.source, e.target));

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

    /// Returns whether there is an edge `source -> target`.
    #[inline]
    pub fn has_edge(&self, source: usize, target: usize) -> bool {
        let edges = self.edges_from(source);
        edges.binary_search_by_key(&target, |e| e.target).is_ok()
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

    /// Get degree of a node
    #[inline]
    pub fn degree(&self, node: usize) -> usize {
        self.degrees.get(node).copied().unwrap_or(0)
    }

    /// Get the number of sorted edges
    #[inline]
    pub fn sorted_edges_len(&self) -> usize {
        self.sorted_edges.len()
    }

    /// Get the number of source indices
    #[inline]
    pub fn source_indices_len(&self) -> usize {
        self.source_indices.len()
    }

    /// Get the number of degrees
    #[inline]
    pub fn degrees_len(&self) -> usize {
        self.degrees.len()
    }
}
