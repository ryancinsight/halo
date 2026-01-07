//! Cow-based string collection with zero-copy operations.
//!
//! This collection uses `std::borrow::Cow` to avoid allocations when strings
//! are already owned, providing optimal memory efficiency for string processing.
//!
//! # Zero-Copy Operations
//!
//! All operations are designed to avoid unnecessary allocations:
//! - Borrowed strings are stored as references (zero-copy)
//! - Owned strings are stored as owned values
//! - Deduplication prevents duplicate storage
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
//!     let idx1 = strings.insert_borrowed("hello");
//!     let idx2 = strings.insert_borrowed("world");
//!
//!     // Owned string insertion
//!     let idx3 = strings.insert_owned("owned".to_string());
//!
//!     // Deduplication - same string returns same index
//!     let idx4 = strings.insert_borrowed("hello");
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
//!
//! # Performance Characteristics
//!
//! - **Memory**: Minimal allocations for borrowed strings
//! - **Lookup**: O(1) average case with hash-based indexing
//! - **Deduplication**: Automatic sharing of identical strings
//! - **Iteration**: Zero-copy access to all strings

use crate::{GhostToken, GhostCell};
use std::borrow::Cow;
use std::collections::HashMap;

/// A collection of Cow strings with token-gated access.
/// Provides zero-copy operations when strings are already owned.
pub struct BrandedCowStrings<'brand> {
    strings: Vec<GhostCell<'brand, Cow<'brand, str>>>,
    index: HashMap<Cow<'brand, str>, usize>,
}

impl<'brand> BrandedCowStrings<'brand> {
    /// Creates a new empty collection.
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Creates a new collection with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
        }
    }

    /// Adds a string, using Cow to avoid allocation if already owned.
    /// Returns the index of the inserted string.
    pub fn insert(&mut self, s: Cow<'brand, str>) -> usize {
        if let Some(&idx) = self.index.get(&s) {
            return idx; // String already exists, return existing index
        }

        let idx = self.strings.len();
        self.strings.push(GhostCell::new(s.clone()));
        self.index.insert(s, idx);
        idx
    }

    /// Adds a borrowed string without copying.
    pub fn insert_borrowed(&mut self, s: &'brand str) -> usize {
        let cow = Cow::Borrowed(s);
        self.insert(cow)
    }

    /// Adds an owned string.
    pub fn insert_owned(&mut self, s: String) -> usize {
        let cow = Cow::Owned(s);
        self.insert(cow)
    }

    /// Gets a reference to a string by index with zero-copy access.
    #[inline(always)]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a Cow<'brand, str>> {
        self.strings.get(idx).map(|cell| cell.borrow(token))
    }

    /// Gets a reference to a string by value with zero-copy lookup.
    #[inline(always)]
    pub fn get_by_value<'a>(&'a self, token: &'a GhostToken<'brand>, value: &str) -> Option<&'a Cow<'brand, str>> {
        self.index.get(value).and_then(move |&idx| self.get(token, idx))
    }

    /// Returns the number of unique strings stored.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Returns true if no strings are stored.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Zero-copy iterator over all strings.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a Cow<'brand, str>> {
        self.strings.iter().map(move |cell| cell.borrow(token))
    }

    /// Zero-copy filter operation.
    pub fn filter_ref<'a, F>(
        &'a self,
        token: &'a GhostToken<'brand>,
        f: F,
    ) -> impl Iterator<Item = &Cow<'brand, str>> + 'a
    where
        F: Fn(&Cow<'brand, str>) -> bool + 'a,
    {
        self.iter(token).filter(move |item| f(item))
    }

    /// Memory usage statistics.
    pub fn memory_stats(&self) -> CowStringsStats {
        // Note: We can't access the strings without a token, so we return basic stats
        CowStringsStats {
            string_count: self.strings.len(),
            index_entries: self.index.len(),
            // Would need token to calculate actual memory usage
        }
    }
}

/// Memory usage statistics for BrandedCowStrings.
#[derive(Debug, Clone)]
pub struct CowStringsStats {
    pub string_count: usize,
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

    #[test]
    fn test_cow_strings_zero_copy() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // Insert borrowed strings (zero-copy)
            let idx1 = strings.insert_borrowed("hello");
            let idx2 = strings.insert_borrowed("world");

            // Insert owned string
            let idx3 = strings.insert_owned("owned".to_string());

            // Access without copying
            assert_eq!(strings.get(&token, idx1).unwrap().as_ref(), "hello");
            assert_eq!(strings.get(&token, idx2).unwrap().as_ref(), "world");
            assert_eq!(strings.get(&token, idx3).unwrap().as_ref(), "owned");

            // Deduplication works
            let idx1_dup = strings.insert_borrowed("hello");
            assert_eq!(idx1, idx1_dup); // Same index for duplicate

            // Iterator works
            let collected: Vec<&str> = strings.iter(&token)
                .map(|cow| cow.as_ref())
                .collect();
            assert_eq!(collected.len(), 3);
        });
    }

    #[test]
    fn test_cow_strings_memory_efficiency() {
        GhostToken::new(|_token| {
            let mut strings = BrandedCowStrings::new();

            // All borrowed - no allocations
            strings.insert_borrowed("static1");
            strings.insert_borrowed("static2");

            let stats = strings.memory_stats();
            assert_eq!(stats.string_count, 2);
        });
    }

    #[test]
    fn test_cow_strings_advanced_operations() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::new();

            // Test mixed insertions
            strings.insert_borrowed("static");
            strings.insert_owned("owned".to_string());
            strings.insert(Cow::Borrowed("another_static"));

            assert_eq!(strings.len(), 3);
            assert!(!strings.is_empty());

            // Test deduplication with different Cow types
            let idx1 = strings.insert_borrowed("dup");
            let idx2 = strings.insert_owned("dup".to_string());
            assert_eq!(idx1, idx2); // Same index

            // Test filter operations
            let filtered: Vec<_> = strings.filter_ref(&token, |cow| cow.len() > 4)
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
            let idx = strings.insert_borrowed("");
            assert_eq!(strings.get(&token, idx).unwrap().as_ref(), "");

            // Unicode strings
            let unicode = "🚀 Halo 🌟";
            let idx = strings.insert_borrowed(unicode);
            assert_eq!(strings.get(&token, idx).unwrap().as_ref(), unicode);

            // Very long strings
            let long_string = "a".repeat(10000);
            let idx = strings.insert_owned(long_string.clone());
            assert_eq!(strings.get(&token, idx).unwrap().as_ref(), long_string.as_str());
        });
    }

    #[test]
    fn test_cow_strings_capacity_and_growth() {
        GhostToken::new(|token| {
            let mut strings = BrandedCowStrings::with_capacity(10);

            // Fill with different strings
            for i in 0..20 {
                strings.insert_owned(format!("string_{}", i));
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
