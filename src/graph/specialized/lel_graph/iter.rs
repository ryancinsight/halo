use crate::graph::compressed::ecc_graph::EccEdge;

/// Iterator over neighbors in LEL graph (targets for a fixed source).
pub struct LelNeighborIter<'a> {
    edges: &'a [EccEdge],
    idx: usize,
}

impl<'a> LelNeighborIter<'a> {
    #[inline]
    pub(super) fn new(edges: &'a [EccEdge]) -> Self {
        Self { edges, idx: 0 }
    }
}

impl<'a> Iterator for LelNeighborIter<'a> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let e = self.edges.get(self.idx)?;
        self.idx += 1;
        Some(e.target)
    }
}
