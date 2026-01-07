use super::iter::AmtNeighborIter;

/// Thresholds for switching between representations.
pub(super) const SPARSE_THRESHOLD: usize = 32;
pub(super) const DENSE_THRESHOLD: usize = 1024;

/// Adaptive representation for a single node's neighborhood.
#[derive(Clone)]
pub(super) enum NodeRepresentation {
    /// CSR-like sparse representation for low-degree nodes.
    Sparse { neighbors: Vec<usize> },
    /// Dense bitset for high-degree nodes in dense graphs.
    Dense { bitset: Vec<u64>, degree: usize },
    /// Sorted array for medium-degree nodes.
    Sorted { neighbors: Vec<usize> },
}

impl NodeRepresentation {
    #[inline]
    pub(super) fn degree(&self) -> usize {
        match self {
            NodeRepresentation::Sparse { neighbors } => neighbors.len(),
            NodeRepresentation::Dense { degree, .. } => *degree,
            NodeRepresentation::Sorted { neighbors } => neighbors.len(),
        }
    }

    #[inline]
    pub(super) fn has_edge(&self, to: usize) -> bool {
        match self {
            NodeRepresentation::Sparse { neighbors } => neighbors.contains(&to),
            NodeRepresentation::Dense { bitset, .. } => {
                let word_idx = to / 64;
                let bit_idx = to % 64;
                bitset
                    .get(word_idx)
                    .map_or(false, |word| (word & (1u64 << bit_idx)) != 0)
            }
            NodeRepresentation::Sorted { neighbors } => neighbors.binary_search(&to).is_ok(),
        }
    }

    #[inline]
    pub(super) fn neighbors<'a>(&'a self, node_count: usize) -> AmtNeighborIter<'a> {
        match self {
            NodeRepresentation::Sparse { neighbors } => AmtNeighborIter::Sparse(neighbors.iter()),
            NodeRepresentation::Dense { bitset, .. } => AmtNeighborIter::Dense {
                bitset,
                index: 0,
                len: node_count,
            },
            NodeRepresentation::Sorted { neighbors } => AmtNeighborIter::Sorted(neighbors.iter()),
        }
    }
}



