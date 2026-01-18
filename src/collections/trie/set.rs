use core::marker::PhantomData;

use crate::GhostToken;
use crate::collections::{BrandedCollection, ZeroCopyOps};
use super::map::BrandedRadixTrieMap;

/// A high-performance Radix Trie Set optimized for branded usage.
///
/// Keys must implement `AsRef<[u8]>`.
pub struct BrandedRadixTrieSet<'brand, T> {
    map: BrandedRadixTrieMap<'brand, T, ()>,
}

impl<'brand, T> BrandedRadixTrieSet<'brand, T> {
    /// Creates a new empty Radix Trie Set.
    pub fn new() -> Self {
        Self {
            map: BrandedRadixTrieMap::new(),
        }
    }

    /// Creates a new empty Radix Trie Set with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: BrandedRadixTrieMap::with_capacity(capacity),
        }
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Clears the set.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Iterates over all elements, passing the value (as slice) to the closure.
    /// This avoids allocating a new Vec for each value.
    pub fn for_each<F>(&self, token: &GhostToken<'brand>, mut f: F)
    where F: FnMut(&[u8])
    {
        self.map.for_each(token, |k, _| f(k));
    }
}

impl<'brand, T> BrandedRadixTrieSet<'brand, T>
where
    T: AsRef<[u8]>,
{
    /// Adds a value to the set.
    /// Returns whether the value was newly inserted.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, value: T) -> bool {
        self.map.insert(token, value, ()).is_none()
    }

    /// Returns true if the set contains the value.
    pub fn contains(&self, token: &GhostToken<'brand>, value: T) -> bool {
        self.map.get(token, value).is_some()
    }

    /// Removes a value from the set.
    /// Returns whether the value was present.
    pub fn remove(&mut self, token: &mut GhostToken<'brand>, value: T) -> bool {
        self.map.remove(token, value).is_some()
    }
}

impl<'brand, T> BrandedCollection<'brand> for BrandedRadixTrieSet<'brand, T> {
    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

// ZeroCopyOps
impl<'brand, T> ZeroCopyOps<'brand, T> for BrandedRadixTrieSet<'brand, T>
where
    T: AsRef<[u8]>,
{
    // Similar to Map, we can't easily return &T because T is not stored as a whole in nodes.
    // The keys are implicit.
    // So find_ref is hard.

    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        None // Placeholder
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        false // Placeholder
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        true // Placeholder
    }
}
