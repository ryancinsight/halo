/// Iterator over neighbors in AMT graph.
pub enum AmtNeighborIter<'a> {
    /// Iterator over a sparse node's neighbors (stored as a list).
    Sparse(std::slice::Iter<'a, usize>),
    /// Iterator over a dense node's neighbors (stored as a bitset).
    Dense {
        /// The bitset containing neighbor information.
        bitset: &'a [u64],
        /// Current index in the bitset.
        index: usize,
        /// Maximum index to check.
        len: usize,
    },
    /// Iterator over a sorted node's neighbors.
    Sorted(std::slice::Iter<'a, usize>),
}

impl<'a> Iterator for AmtNeighborIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            AmtNeighborIter::Sparse(iter) => iter.next().copied(),
            AmtNeighborIter::Dense { bitset, index, len } => {
                while *index < *len {
                    let word_idx = *index / 64;
                    let bit_idx = *index % 64;

                    if word_idx < bitset.len() {
                        let word = bitset[word_idx];
                        if (word & (1u64 << bit_idx)) != 0 {
                            let result = *index;
                            *index += 1;
                            return Some(result);
                        }
                    }
                    *index += 1;
                }
                None
            }
            AmtNeighborIter::Sorted(iter) => iter.next().copied(),
        }
    }
}
