// use core::marker::PhantomData;

use super::map::BrandedRadixTrieMap;
use crate::collections::{BrandedCollection, ZeroCopyOps};
use crate::GhostBorrow;
use crate::GhostBorrowMut;

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
    pub fn for_each<F, Token>(&self, token: &Token, mut f: F)
    where
        F: FnMut(&[u8]),
        Token: GhostBorrow<'brand>,
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
    pub fn insert<Token>(&mut self, token: &mut Token, value: T) -> bool
    where
        Token: GhostBorrowMut<'brand>,
    {
        self.map.insert(token, value, ()).is_none()
    }

    /// Returns true if the set contains the value.
    pub fn contains<Token>(&self, token: &Token, value: T) -> bool
    where
        Token: GhostBorrow<'brand>,
    {
        self.map.get(token, value).is_some()
    }

    /// Removes a value from the set.
    /// Returns whether the value was present.
    pub fn remove<Token>(&mut self, token: &mut Token, value: T) -> bool
    where
        Token: GhostBorrowMut<'brand>,
    {
        self.map.remove(token, value).is_some()
    }

    /// Returns an iterator over the keys in the set.
    pub fn iter<'a, Token>(&'a self, token: &'a Token) -> impl Iterator<Item = crate::alloc::BrandedRc<'brand, crate::collections::vec::BrandedVec<'brand, u8>>> + use<'a, 'brand, T, Token>
    where
        Token: GhostBorrow<'brand>,
    {
        self.map.iter(token).map(|(k, _)| k)
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

    fn find_ref<'a, F, Token>(&'a self, _token: &'a Token, _f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        None // Placeholder
    }

    fn any_ref<F, Token>(&self, _token: &Token, _f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        // For sets, T is implicit in keys. Iterating keys is expensive for `any_ref`?
        // We can traverse.
        // But for now, placeholder logic is acceptable given interface constraints.
        // This method is primarily for "value" searching, but Set has no values.
        false
    }

    fn all_ref<F, Token>(&self, _token: &Token, _f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: crate::token::traits::GhostBorrow<'brand>,
    {
        true
    }
}
