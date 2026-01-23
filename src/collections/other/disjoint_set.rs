//! Disjoint Set (Union-Find) implementation with token-gated safety.
//!
//! This implementation uses `GhostToken` to guarantee safety while allowing
//! path compression (interior mutability) without the runtime overhead of `RefCell`.
//!
//! # Performance
//!
//! - Uses `Cell<usize>` for parent pointers to enable zero-cost path compression.
//! - Uses `BrandedVec` for storage, ensuring cache locality.
//! - Path compression and union-by-rank ensure nearly constant time operations.

use crate::collections::BrandedVec;
use crate::GhostToken;
use std::cell::Cell;

/// A Disjoint Set (Union-Find) data structure.
pub struct BrandedDisjointSet<'brand> {
    /// Parent pointers.
    /// Uses `Cell` to allow path compression with shared reference.
    parent: BrandedVec<'brand, Cell<usize>>,
    /// Rank (depth upper bound) for union-by-rank.
    rank: BrandedVec<'brand, u8>,
}

impl<'brand> BrandedDisjointSet<'brand> {
    /// Creates a new empty disjoint set.
    pub fn new() -> Self {
        Self {
            parent: BrandedVec::new(),
            rank: BrandedVec::new(),
        }
    }

    /// Creates a new disjoint set with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            parent: BrandedVec::with_capacity(capacity),
            rank: BrandedVec::with_capacity(capacity),
        }
    }

    /// Creates a new set containing a single element.
    /// Returns the representative ID of the new set.
    pub fn make_set(&mut self, _token: &mut GhostToken<'brand>) -> usize {
        let id = self.parent.len();
        self.parent.push(Cell::new(id));
        self.rank.push(0);
        id
    }

    /// Finds the representative of the set containing `id`, with path compression.
    ///
    /// This operation is "logically const" but performs internal mutation (path compression).
    /// Thanks to `Cell` and branding, this is safe with a shared `GhostToken`.
    pub fn find(&self, token: &GhostToken<'brand>, id: usize) -> usize {
        // Two-pass approach for path compression:
        // 1. Find root
        let mut root = id;
        loop {
            // Safety: We assume id is valid. If not, get() panics or returns None.
            // But we use get() which returns Option.
            // We expect standard usage where IDs are valid.
            let parent_cell = self.parent.get(token, root).expect("index out of bounds");
            let parent = parent_cell.get();
            if parent == root {
                break;
            }
            root = parent;
        }

        // 2. Compress path
        let mut curr = id;
        while curr != root {
            let parent_cell = self.parent.get(token, curr).unwrap();
            let parent = parent_cell.get();
            parent_cell.set(root);
            curr = parent;
        }

        root
    }

    /// Unites the sets containing `id1` and `id2`.
    /// Returns `true` if they were in different sets, `false` otherwise.
    ///
    /// Requires `&mut GhostToken` because it modifies the structure (union).
    pub fn union(&mut self, token: &mut GhostToken<'brand>, id1: usize, id2: usize) -> bool {
        let root1 = self.find(token, id1);
        let root2 = self.find(token, id2);

        if root1 == root2 {
            return false;
        }

        // Union by rank
        let rank1 = *self.rank.borrow(token, root1);
        let rank2 = *self.rank.borrow(token, root2);

        if rank1 < rank2 {
            // Attach 1 to 2
            self.parent.borrow(token, root1).set(root2);
        } else if rank1 > rank2 {
            // Attach 2 to 1
            self.parent.borrow(token, root2).set(root1);
        } else {
            // Same rank, attach 2 to 1 and increment rank of 1
            self.parent.borrow(token, root2).set(root1);
            *self.rank.borrow_mut(token, root1) += 1;
        }

        true
    }

    /// Returns the number of elements in the disjoint set.
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }
}

impl<'brand> Default for BrandedDisjointSet<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

/// A wrapper around `BrandedDisjointSet` that bundles the token for convenience.
pub struct ActiveDisjointSet<'a, 'brand> {
    inner: &'a mut BrandedDisjointSet<'brand>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand> ActiveDisjointSet<'a, 'brand> {
    /// Creates a new active disjoint set.
    pub fn new(
        inner: &'a mut BrandedDisjointSet<'brand>,
        token: &'a mut GhostToken<'brand>,
    ) -> Self {
        Self { inner, token }
    }

    /// Creates a new set.
    pub fn make_set(&mut self) -> usize {
        self.inner.make_set(self.token)
    }

    /// Finds the representative.
    pub fn find(&mut self, id: usize) -> usize {
        self.inner.find(self.token, id)
    }

    /// Unites two sets.
    pub fn union(&mut self, id1: usize, id2: usize) -> bool {
        self.inner.union(self.token, id1, id2)
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disjoint_set() {
        GhostToken::new(|mut token| {
            let mut ds = BrandedDisjointSet::new();

            let a = ds.make_set(&mut token);
            let b = ds.make_set(&mut token);
            let c = ds.make_set(&mut token);

            assert_eq!(ds.find(&token, a), a);
            assert_eq!(ds.find(&token, b), b);

            assert!(ds.union(&mut token, a, b));
            assert_eq!(ds.find(&token, a), ds.find(&token, b));
            assert_ne!(ds.find(&token, a), ds.find(&token, c));

            assert!(ds.union(&mut token, b, c));
            assert_eq!(ds.find(&token, a), ds.find(&token, c));

            // Already united
            assert!(!ds.union(&mut token, a, c));
        });
    }

    #[test]
    fn test_active_disjoint_set() {
        GhostToken::new(|mut token| {
            let mut ds = BrandedDisjointSet::new();
            let mut active = ActiveDisjointSet::new(&mut ds, &mut token);

            let a = active.make_set();
            let b = active.make_set();

            assert!(active.union(a, b));
            assert_eq!(active.find(a), active.find(b));
        });
    }
}
