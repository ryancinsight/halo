/// Iterator over neighbors in AMT graph.
pub enum AmtNeighborIter<'a> {
    Sparse(std::slice::Iter<'a, usize>),
    Dense {
        bitset: &'a [u64],
        index: usize,
        len: usize,
    },
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
