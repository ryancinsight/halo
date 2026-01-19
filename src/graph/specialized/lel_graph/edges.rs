use crate::graph::compressed::ecc_graph::EccEdge;

/// Sorted edge list for LEL representation, with per-source boundaries.
#[derive(Clone, Debug)]
pub struct DeltaEncodedEdges {
    pub(super) sorted_edges: Vec<EccEdge>,
    pub(super) source_indices: Vec<usize>,
}

impl DeltaEncodedEdges {
    /// Create sorted edge list from edges, building `source_indices` for `node_count`.
    pub fn from_edges(node_count: usize, edges: &[EccEdge]) -> Self {
        let mut sorted_edges = edges.to_vec();
        // Sort by (source, target) so per-source neighbor lists are sorted.
        sorted_edges.sort_unstable_by_key(|e| (e.source, e.target));

        let mut source_indices = vec![0usize; node_count + 1];
        let mut current_source = 0usize;
        for (i, e) in sorted_edges.iter().enumerate() {
            while current_source <= e.source && current_source < source_indices.len() {
                source_indices[current_source] = i;
                current_source += 1;
            }
        }
        while current_source < source_indices.len() {
            source_indices[current_source] = sorted_edges.len();
            current_source += 1;
        }

        Self {
            sorted_edges,
            source_indices,
        }
    }

    #[inline]
    pub fn edges_from(&self, source: usize) -> &[EccEdge] {
        if source + 1 >= self.source_indices.len() {
            return &[];
        }
        let start = self.source_indices[source];
        let end = self.source_indices[source + 1];
        &self.sorted_edges[start..end]
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.sorted_edges.len()
    }
}
