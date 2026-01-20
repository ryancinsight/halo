//! `BrandedInterner` — a generic zero-copy interner with token-gated access.
//!
//! This collection allows interning any `Hash + Eq + Clone` type, ensuring that only
//! one copy of each unique value is stored. It uses a `BrandedVec` for storage and
//! a custom index-based hash table for lookups, providing **significant memory savings**
//! by not duplicating keys in the hash map.
//!
//! # Features
//! - **Zero-Copy Lookup**: Find values without cloning or allocating.
//! - **Memory Efficient**: Stores values only once; hash table only stores indices and hashes.
//! - **Token-Gated**: Uses `GhostToken` to ensure safe access to the interned values.
//! - **Stable Indices**: Interned values are never moved or removed (append-only), providing stable `InternId`s.

use crate::collections::{BrandedCollection, BrandedVec};
use crate::GhostToken;
use std::borrow::Cow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;

/// A handle to an interned value.
///
/// This handle is a lightweight wrapper around an index, ensuring that it
/// can only be resolved by the `BrandedInterner` that created it (checked via `'brand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternId<'brand> {
    index: u32,
    _marker: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> InternId<'brand> {
    #[inline(always)]
    fn new(index: usize) -> Self {
        debug_assert!(
            index <= u32::MAX as usize,
            "Interner index overflow: too many interned items"
        );
        Self {
            index: index as u32,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying index.
    #[inline(always)]
    pub fn index(&self) -> usize {
        self.index as usize
    }
}

/// Entry in the hash table.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// Cached hash of the key to speed up probing and resizing.
    hash: u64,
    /// Index into the `storage` vector.
    index: usize,
}

/// A generic interner with token-gated access.
pub struct BrandedInterner<'brand, T, S = RandomState> {
    /// Backing storage for values.
    storage: BrandedVec<'brand, T>,
    /// Hash table mapping hash -> index.
    /// Uses open addressing with linear probing.
    /// Size is always a power of 2.
    buckets: Vec<Option<Entry>>,
    /// Number of elements in the map.
    len: usize,
    /// Hash builder.
    hash_builder: S,
}

impl<'brand, T> BrandedInterner<'brand, T, RandomState> {
    /// Creates a new empty interner.
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new interner with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<'brand, T, S> BrandedInterner<'brand, T, S> {
    /// Creates a new interner with capacity and hasher.
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        let cap = if capacity < 4 {
            4
        } else {
            capacity.next_power_of_two()
        };
        Self {
            storage: BrandedVec::with_capacity(capacity),
            buckets: vec![None; cap],
            len: 0,
            hash_builder,
        }
    }
}

impl<'brand, T, S> BrandedInterner<'brand, T, S>
where
    T: Hash + Eq + Clone,
    S: BuildHasher,
{
    /// Computes the hash of a value.
    fn hash_val<Q: ?Sized>(&self, s: &Q) -> u64
    where
        T: std::borrow::Borrow<Q>,
        Q: Hash,
    {
        let mut hasher = self.hash_builder.build_hasher();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Helper to find a slot for a given key.
    /// Returns `Ok(index)` if found, `Err(slot_index)` if not found (where to insert).
    fn find_slot<Q: ?Sized>(
        &self,
        token: &GhostToken<'brand>,
        key: &Q,
        hash: u64,
    ) -> Result<usize, usize>
    where
        T: std::borrow::Borrow<Q>,
        Q: Eq,
    {
        let mask = self.buckets.len() - 1;
        let mut idx = (hash as usize) & mask;
        let mut dist = 0;

        loop {
            match self.buckets[idx] {
                None => return Err(idx),
                Some(entry) => {
                    if entry.hash == hash {
                        // Potential match, verify with token
                        // SAFETY: entry.index is valid because we only insert valid indices
                        if let Some(stored_val) = self.storage.get(token, entry.index) {
                            if stored_val.borrow() == key {
                                return Ok(entry.index);
                            }
                        }
                    }
                }
            }
            idx = (idx + 1) & mask;
            dist += 1;
            if dist >= self.buckets.len() {
                // Table is full, should have resized before this.
                return Err(idx);
            }
        }
    }

    /// Resizes the hash table.
    fn resize(&mut self) {
        let new_cap = self.buckets.len() * 2;
        let mut new_buckets = vec![None; new_cap];
        let mask = new_cap - 1;

        for entry in self.buckets.iter().flatten() {
            let mut idx = (entry.hash as usize) & mask;
            while new_buckets[idx].is_some() {
                idx = (idx + 1) & mask;
            }
            new_buckets[idx] = Some(*entry);
        }

        self.buckets = new_buckets;
    }

    /// Interns a value.
    ///
    /// If the value already exists, returns its `InternId`.
    /// If not, inserts it and returns a new `InternId`.
    pub fn intern(&mut self, token: &GhostToken<'brand>, value: T) -> InternId<'brand> {
        self.intern_cow(token, Cow::Owned(value))
    }

    /// Interns a value from a Cow reference.
    ///
    /// This allows avoiding allocation if the value is already present.
    pub fn intern_cow<'a>(
        &mut self,
        token: &GhostToken<'brand>,
        value: Cow<'a, T>,
    ) -> InternId<'brand> {
        let hash = self.hash_val(value.as_ref());

        // Check load factor (75%)
        if self.len * 4 > self.buckets.len() * 3 {
            self.resize();
        }

        match self.find_slot(token, value.as_ref(), hash) {
            Ok(idx) => InternId::new(idx),
            Err(slot) => {
                let idx = self.storage.len();
                self.storage.push(value.into_owned());
                self.buckets[slot] = Some(Entry { hash, index: idx });
                self.len += 1;
                InternId::new(idx)
            }
        }
    }

    /// Gets a reference to an interned value by ID.
    #[inline(always)]
    pub fn get<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        id: InternId<'brand>,
    ) -> Option<&'a T> {
        self.storage.get(token, id.index())
    }

    /// Looks up a value by reference without allocating.
    ///
    /// Returns the `InternId` if found.
    pub fn get_id<Q: ?Sized>(&self, token: &GhostToken<'brand>, key: &Q) -> Option<InternId<'brand>>
    where
        T: std::borrow::Borrow<Q>,
        Q: Hash + Eq,
    {
        let hash = self.hash_val(key);
        match self.find_slot(token, key, hash) {
            Ok(idx) => Some(InternId::new(idx)),
            Err(_) => None,
        }
    }

    /// Looks up a value by reference and returns a reference to the stored value.
    ///
    /// This is useful for canonicalizing values (replacing a lookup key with the stored canonical version).
    pub fn get_val<'a, Q: ?Sized>(
        &'a self,
        token: &'a GhostToken<'brand>,
        key: &Q,
    ) -> Option<&'a T>
    where
        T: std::borrow::Borrow<Q>,
        Q: Hash + Eq,
    {
        let hash = self.hash_val(key);
        match self.find_slot(token, key, hash) {
            Ok(idx) => self.storage.get(token, idx),
            Err(_) => None,
        }
    }

    /// Iterates over all interned values.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = (InternId<'brand>, &'a T)> {
        self.storage
            .iter(token)
            .enumerate()
            .map(|(i, v)| (InternId::new(i), v))
    }
}

impl<'brand, T, S> BrandedCollection<'brand> for BrandedInterner<'brand, T, S> {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<'brand, T, S> Default for BrandedInterner<'brand, T, S>
where
    T: Hash + Eq + Clone,
    S: BuildHasher + Default,
{
    fn default() -> Self {
        Self::with_capacity_and_hasher(0, S::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_interner_basic() {
        GhostToken::new(|token| {
            let mut interner = BrandedInterner::new();

            let id1 = interner.intern(&token, "hello".to_string());
            let id2 = interner.intern(&token, "world".to_string());
            let id3 = interner.intern(&token, "hello".to_string());

            assert_eq!(id1, id3);
            assert_ne!(id1, id2);

            assert_eq!(interner.get(&token, id1), Some(&"hello".to_string()));
            assert_eq!(interner.get(&token, id2), Some(&"world".to_string()));
        });
    }

    #[test]
    fn test_interner_lookup() {
        GhostToken::new(|token| {
            let mut interner = BrandedInterner::new();
            interner.intern(&token, "test".to_string());

            let found = interner.get_val(&token, "test");
            assert_eq!(found, Some(&"test".to_string()));

            let not_found = interner.get_val(&token, "missing");
            assert_eq!(not_found, None);
        });
    }

    #[test]
    fn test_interner_types() {
        GhostToken::new(|token| {
            let mut interner = BrandedInterner::new();

            // Intern integers
            let id1 = interner.intern(&token, 42);
            let id2 = interner.intern(&token, 42);

            assert_eq!(id1, id2);
            assert_eq!(interner.get(&token, id1), Some(&42));
        });
    }
}
