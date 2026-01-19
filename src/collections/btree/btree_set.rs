//! `BrandedBTreeSet` — a B-Tree set with token-gated access.
//!
//! Implemented as a wrapper around `BrandedBTreeMap`.

use crate::{GhostCell, GhostToken};
use super::btree_map::BrandedBTreeMap;
use std::borrow::Borrow;

/// A B-Tree set.
pub struct BrandedBTreeSet<'brand, T> {
    map: BrandedBTreeMap<'brand, T, ()>,
}

impl<'brand, T> BrandedBTreeSet<'brand, T> {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self {
            map: BrandedBTreeMap::new(),
        }
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<'brand, T> BrandedBTreeSet<'brand, T>
where
    T: Ord,
{
    /// Adds a value to the set.
    /// Returns whether the value was newly inserted.
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }

    /// Returns `true` if the set contains the value.
    pub fn contains<Q: ?Sized>(&self, token: &GhostToken<'brand>, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.contains_key_with_token(token, value)
    }

    /// Removes a value from the set.
    /// Returns whether the value was present.
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Ord,
    {
        self.map.remove(value).is_some()
    }

    /// Returns an iterator over the values in the set.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> super::btree_map::Keys<'a, 'brand, T, ()> {
        self.map.keys(token)
    }
}

impl<'brand, T> Default for BrandedBTreeSet<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> crate::collections::BrandedCollection<'brand> for BrandedBTreeSet<'brand, T> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_set_ops() {
        GhostToken::new(|token| {
            let mut set = BrandedBTreeSet::new();
            assert!(set.is_empty());

            assert!(set.insert(1));
            assert!(!set.insert(1));
            assert_eq!(set.len(), 1);

            assert!(set.contains(&token, &1));
            assert!(!set.contains(&token, &2));

            assert!(set.insert(2));
            assert!(set.insert(3));
            assert_eq!(set.len(), 3);

            let mut items: Vec<i32> = set.iter(&token).copied().collect();
            items.sort();
            assert_eq!(items, vec![1, 2, 3]);

            assert!(set.remove(&2));
            assert!(!set.contains(&token, &2));
            assert_eq!(set.len(), 2);
        });
    }
}
