//! Iterator implementations for compressed graph.

/// Iterator over neighbors in compressed graph
pub struct CompressedNeighborIter<'a> {
    edges: &'a [usize],
    index: usize,
    end: usize,
}

impl<'a> CompressedNeighborIter<'a> {
    #[inline]
    pub fn new(edges: &'a [usize], start: usize, end: usize) -> Self {
        Self {
            edges,
            index: start,
            end,
        }
    }
}

impl<'a> Iterator for CompressedNeighborIter<'a> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.end {
            None
        } else {
            let result = self.edges[self.index];
            self.index += 1;
            Some(result)
        }
    }
}
