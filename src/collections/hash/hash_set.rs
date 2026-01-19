//! `BrandedHashSet` — a hash set with token-gated membership (informally).
//!
//! Membership is tracked by presence in the set. Values (keys) are stored
//! plainly for hashing, but since it's a branded set, we could conceptually
//! protect membership tests or similar. Here we implement it as a thin
//! wrapper over `BrandedHashMap<K, ()>`.

use super::hash_map::BrandedHashMap;
use crate::GhostToken;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

/// A hash set with branded membership.
#[repr(transparent)]
pub struct BrandedHashSet<'brand, K, S = RandomState> {
    inner: BrandedHashMap<'brand, K, (), S>,
}

impl<'brand, K> BrandedHashSet<'brand, K, RandomState>
where
    K: Eq + Hash,
{
    /// Creates an empty set.
    pub fn new() -> Self {
        Self {
            inner: BrandedHashMap::new(),
        }
    }

    /// Creates an empty set with at least the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: BrandedHashMap::with_capacity(capacity),
        }
    }
}

impl<'brand, K, S> BrandedHashSet<'brand, K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Inserts a value. Returns `true` if it was not already present.
    pub fn insert(&mut self, value: K) -> bool {
        self.inner.insert(value, ()).is_none()
    }

    /// Removes a value from the set. Returns `true` if it was present.
    pub fn remove(&mut self, value: &K) -> bool {
        self.inner.remove(value).is_some()
    }

    /// Returns `true` if the set contains the value.
    ///
    /// Note: Does not strictly need a token for existence, but we keep it
    /// consistent with the map API if we want to gate "observation" of the set.
    /// However, keys are not in cells, so we just provide a normal `contains`.
    pub fn contains(&self, value: &K) -> bool {
        self.inner.contains_key(value)
    }

    /// Returns `true` if the set contains the value (token-gated version).
    pub fn contains_gated(&self, token: &GhostToken<'brand>, value: &K) -> bool {
        self.inner.get(token, value).is_some()
    }

    /// Iterates over all values.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Bulk operation: applies `f` to all keys.
    ///
    /// Note: Keys are not token-gated since they are used for external access,
    /// but this provides a consistent bulk API.
    #[inline]
    pub fn for_each_key<F>(&self, mut f: F)
    where
        F: FnMut(&K),
    {
        for key in self.inner.keys() {
            f(key);
        }
    }
}

impl<'brand, K> Default for BrandedHashSet<'brand, K, RandomState>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_hash_set_basic() {
        GhostToken::new(|token| {
            let mut set = BrandedHashSet::new();
            set.insert("a");
            set.insert("b");

            assert_eq!(set.len(), 2);
            assert!(set.contains(&"a"));
            assert!(set.contains_gated(&token, &"a"));
            assert!(set.contains(&"b"));
            assert!(!set.contains(&"c"));
        });
    }
}
