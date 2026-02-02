//! Cow-based string collection with zero-copy operations and optimized storage.
//!
//! This collection uses `std::borrow::Cow` to avoid allocations when strings
//! are already owned, providing optimal memory efficiency for string processing.
//!
//! # Optimization: Token-Gated Interning
//!
//! This implementation uses a custom index-based hash table that relies on `GhostToken`
//! to verify equality against stored strings. This eliminates the need to store a
//! duplicate copy of the string in the hash map keys (which `std::collections::HashMap` would require),
//! resulting in **~50% memory savings** for owned strings compared to standard approaches.
//!
//! # Zero-Copy Operations
//!
//! All operations are designed to avoid unnecessary allocations:
//! - Borrowed strings are stored as references (zero-copy)
//! - Owned strings are stored as owned values (deduplicated)
//! - **No duplication**: Strings are stored only once in the backing vector
//! - Iteration provides direct access without copying
//!
//! # Examples
//!
//! ```
//! use halo::{GhostToken, BrandedCowStrings};
//! use std::borrow::Cow;
//!
//! GhostToken::new(|token| {
//!     let mut strings = BrandedCowStrings::new();
//!
//!     // Zero-copy insertion of borrowed strings
//!     // Note: insert now requires token for equality check
//!     let idx1 = strings.insert(&token, Cow::Borrowed("hello"));
//!     let idx2 = strings.insert(&token, Cow::Borrowed("world"));
//!
//!     // Owned string insertion
//!     let idx3 = strings.insert(&token, Cow::Owned("owned".to_string()));
//!
//!     // Deduplication - same string returns same index
//!     let idx4 = strings.insert(&token, Cow::Borrowed("hello"));
//!     assert_eq!(idx1, idx4);
//!
//!     // Zero-copy access
//!     assert_eq!(strings.get(&token, idx1).unwrap().as_ref(), "hello");
//!
//!     // Efficient iteration
//!     let collected: Vec<&str> = strings.iter(&token)
//!         .map(|cow| cow.as_ref())
//!         .collect();
//!     assert_eq!(collected, vec!["hello", "world", "owned"]);
//! });
//! ```

use crate::collections::BrandedVec;
use crate::token::traits::GhostBorrow;
use std::borrow::Cow;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};

/// Entry in the hash table.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// Cached hash of the key to speed up probing and resizing.
    hash: u64,
    /// Index into the `strings` vector.
    index: usize,
}

/// A collection of Cow strings with token-gated access.
/// Provides zero-copy operations when strings are already owned.
pub struct BrandedCowStrings<'brand> {
    /// Backing storage for strings.
    strings: BrandedVec<'brand, Cow<'brand, str>>,
    /// Hash table mapping hash -> index.
    /// Uses open addressing with linear probing.
    /// Size is always a power of 2.
    buckets: Vec<Option<Entry>>,
    /// Number of elements in the map.
    len: usize,
    /// Hash state for computing hashes.
    hash_builder: RandomState,
}

impl<'brand> BrandedCowStrings<'brand> {
    /// Creates a new empty collection.
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Creates a new collection with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = if capacity < 4 {
            4
        } else {
            capacity.next_power_of_two()
        };
        Self {
            strings: BrandedVec::with_capacity(capacity),
            buckets: vec![None; cap],
            len: 0,
            hash_builder: RandomState::new(),
        }
    }

    /// Computes the hash of a string.
    fn hash_str(&self, s: &str) -> u64 {
        let mut hasher = self.hash_builder.build_hasher();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Helper to find a slot for a given key.
    /// Returns `Ok(index)` if found, `Err(slot_index)` if not found (where to insert).
    fn find_slot<Token>(&self, token: &Token, s: &str, hash: u64) -> Result<usize, usize>
    where
        Token: GhostBorrow<'brand>,
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
                        // SAFETY: entry.index is a valid index into self.strings
                        // because we only insert valid indices and never remove strings (append-only).
                        let stored_cow = unsafe { self.strings.get_unchecked(token, entry.index) };
                        if stored_cow == s {
                            return Ok(entry.index);
                        }
                    }
                }
            }
            idx = (idx + 1) & mask;
            dist += 1;
            if dist >= self.buckets.len() {
                // Table is full, should have resized before this.
                // But for safety:
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

    /// Adds a string, using Cow to avoid allocation if already owned.
    /// Returns the index of the inserted string.
    ///
    /// Requires `&GhostToken` to verify uniqueness against stored strings.
    pub fn insert<Token>(&mut self, token: &Token, s: Cow<'brand, str>) -> usize
    where
        Token: GhostBorrow<'brand>,
    {
        let hash = self.hash_str(&s);

        // Check load factor (75%)
        if self.len * 4 > self.buckets.len() * 3 {
            self.resize();
        }

        match self.find_slot(token, &s, hash) {
            Ok(idx) => idx, // Already exists
            Err(slot) => {
                let idx = self.strings.len();
                self.strings.push(s);
                self.buckets[slot] = Some(Entry { hash, index: idx });
                self.len += 1;
                idx
            }
        }
    }

    /// Adds a borrowed string without copying.
    pub fn insert_borrowed<Token>(&mut self, token: &Token, s: &'brand str) -> usize
    where
        Token: GhostBorrow<'brand>,
    {
        self.insert(token, Cow::Borrowed(s))
    }

    /// Adds an owned string.
    pub fn insert_owned<Token>(&mut self, token: &Token, s: String) -> usize
    where
        Token: GhostBorrow<'brand>,
    {
        self.insert(token, Cow::Owned(s))
    }

    /// Gets a reference to a string by index with zero-copy access.
    #[inline(always)]
    pub fn get<'a, Token>(
        &'a self,
        token: &'a Token,
        idx: usize,
    ) -> Option<&'a Cow<'brand, str>>
    where
        Token: GhostBorrow<'brand>,
    {
        self.strings.get(token, idx)
    }

    /// Gets a reference to a string by value with zero-copy lookup.
    #[inline(always)]
    pub fn get_by_value<'a, Token>(
        &'a self,
        token: &'a Token,
        value: &str,
    ) -> Option<&'a Cow<'brand, str>>
    where
        Token: GhostBorrow<'brand>,
    {
        let hash = self.hash_str(value);
        match self.find_slot(token, value, hash) {
            Ok(idx) => self.get(token, idx),
            Err(_) => None,
        }
    }

    /// Returns the number of unique strings stored.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if no strings are stored.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Zero-copy iterator over all strings.
    pub fn iter<'a, Token>(
        &'a self,
        token: &'a Token,
    ) -> impl Iterator<Item = &'a Cow<'brand, str>>
    where
        Token: GhostBorrow<'brand>,
    {
        self.strings.iter(token)
    }

    /// Zero-copy filter operation.
    pub fn filter_ref<'a, F, Token>(
        &'a self,
        token: &'a Token,
        f: F,
    ) -> impl Iterator<Item = &'a Cow<'brand, str>> + 'a
    where
        F: Fn(&Cow<'brand, str>) -> bool + 'a,
        Token: GhostBorrow<'brand>,
    {
        self.iter(token).filter(move |item| f(item))
    }

    /// Memory usage statistics.
    pub fn memory_stats(&self) -> CowStringsStats {
        CowStringsStats {
            string_count: self.len,
            index_entries: self.buckets.len(),
        }
    }
}

/// Memory usage statistics for BrandedCowStrings.
#[derive(Debug, Clone)]
pub struct CowStringsStats {
    /// Number of unique strings stored.
    pub string_count: usize,
    /// Total number of entries in the hash table (including empty buckets if any? No, buckets.len() is capacity).
    /// This field seems to represent `buckets.len()` based on usage.
    pub index_entries: usize,
}

impl Default for BrandedCowStrings<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_cow_strings_zero_copy() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // Insert borrowed strings (zero-copy)
            let idx1 = strings.insert_borrowed(&token, "hello");
            let idx2 = strings.insert_borrowed(&token, "world");

            // Insert owned string
            let idx3 = strings.insert_owned(&token, "owned".to_string());

            // Access without copying
            assert_eq!(strings.get(&token, idx1).unwrap().as_ref(), "hello");
            assert_eq!(strings.get(&token, idx2).unwrap().as_ref(), "world");
            assert_eq!(strings.get(&token, idx3).unwrap().as_ref(), "owned");

            // Deduplication works
            let idx1_dup = strings.insert_borrowed(&token, "hello");
            assert_eq!(idx1, idx1_dup); // Same index for duplicate

            // Iterator works
            let collected: Vec<&str> = strings.iter(&token).map(|cow| cow.as_ref()).collect();
            assert_eq!(collected.len(), 3);
        });
    }

    #[test]
    fn test_cow_strings_memory_efficiency() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // All borrowed - no allocations
            strings.insert_borrowed(&token, "static1");
            strings.insert_borrowed(&token, "static2");

            let stats = strings.memory_stats();
            assert_eq!(stats.string_count, 2);
        });
    }

    #[test]
    fn test_cow_strings_advanced_operations() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // Test mixed insertions
            strings.insert_borrowed(&token, "static");
            strings.insert_owned(&token, "owned".to_string());
            strings.insert(&token, Cow::Borrowed("another_static"));

            assert_eq!(strings.len(), 3);
            assert!(!strings.is_empty());

            // Test deduplication with different Cow types
            let idx1 = strings.insert_borrowed(&token, "dup");
            let idx2 = strings.insert_owned(&token, "dup".to_string());
            assert_eq!(idx1, idx2); // Same index

            // Test filter operations
            let filtered: Vec<_> = strings
                .filter_ref(&token, |cow| cow.len() > 4)
                .map(|cow| cow.as_ref())
                .collect();
            assert_eq!(filtered, vec!["static", "owned", "another_static"]);

            // Test get_by_value
            let found_idx = strings.get_by_value(&token, "owned");
            assert_eq!(found_idx.unwrap().as_ref(), "owned");

            let not_found = strings.get_by_value(&token, "nonexistent");
            assert!(not_found.is_none());
        });
    }

    #[test]
    fn test_cow_strings_edge_cases() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // Empty strings
            let idx = strings.insert_borrowed(&token, "");
            assert_eq!(strings.get(&token, idx).unwrap().as_ref(), "");

            // Unicode strings
            let unicode = "🚀 Halo 🌟";
            let idx = strings.insert_borrowed(&token, unicode);
            assert_eq!(strings.get(&token, idx).unwrap().as_ref(), unicode);

            // Very long strings
            let long_string = "a".repeat(10000);
            let idx = strings.insert_owned(&token, long_string.clone());
            assert_eq!(
                strings.get(&token, idx).unwrap().as_ref(),
                long_string.as_str()
            );
        });
    }

    #[test]
    fn test_cow_strings_capacity_and_growth() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::with_capacity(10);

            // Fill with different strings
            for i in 0..20 {
                strings.insert_owned(&token, format!("string_{}", i));
            }

            assert_eq!(strings.len(), 20);

            // All should be accessible
            for i in 0..20 {
                let value = strings.get_by_value(&token, &format!("string_{}", i));
                assert_eq!(value.unwrap().as_ref(), format!("string_{}", i));
            }
        });
    }
}
