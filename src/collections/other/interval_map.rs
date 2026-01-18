//! `BrandedIntervalMap` — a map of disjoint intervals with token-gated access.
//!
//! Stores a set of disjoint intervals `[start, end)` mapping to values `V`.
//! Supports point lookups and range iteration.
//!
//! Implementation details:
//! - Uses `BrandedVec` for storage, keeping intervals sorted by start coordinate.
//! - Uses `GhostToken` to ensure safe access to the underlying storage.
//! - Zero-copy iteration over intervals.

use crate::GhostToken;
use crate::collections::{BrandedVec, BrandedCollection};
use core::cmp::Ordering;
use core::fmt::Debug;

/// An entry in the interval map representing the range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval<K, V> {
    pub start: K,
    pub end: K,
    pub value: V,
}

/// A map of disjoint intervals.
pub struct BrandedIntervalMap<'brand, K, V> {
    entries: BrandedVec<'brand, Interval<K, V>>,
}

impl<'brand, K, V> BrandedIntervalMap<'brand, K, V>
where
    K: Ord + Copy + Debug,
{
    /// Creates a new empty interval map.
    pub fn new() -> Self {
        Self {
            entries: BrandedVec::new(),
        }
    }

    /// Creates a new interval map with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: BrandedVec::with_capacity(capacity),
        }
    }

    /// Returns the number of intervals in the map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears the map.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Inserts an interval `[start, end)` with `value`.
    ///
    /// This implementation enforces disjoint intervals. If the new interval overlaps
    /// with existing ones, the overlapping parts of existing intervals are overwritten.
    ///
    /// Requires `&mut GhostToken` because it modifies the structure.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, start: K, end: K, value: V)
    where
        V: Clone + PartialEq,
    {
        if start >= end {
            return;
        }

        // 1. Remove/Truncate overlapping intervals
        // This is a complex operation (O(N) in worst case due to vector shifts).
        // Since we are "from scratch", we can do it efficiently.

        // We find the range of indices that overlap.
        let slice = self.entries.as_slice(token);

        // Find first interval that ends > start
        let first_idx = match slice.binary_search_by(|entry| {
            if entry.end <= start {
                Ordering::Less
            } else if entry.start >= end {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }) {
            Ok(idx) => {
                // binary_search finds *any* match. We need the first one.
                let mut i = idx;
                while i > 0 && slice[i-1].end > start {
                    i -= 1;
                }
                i
            }
            Err(idx) => idx, // No overlap, insert at idx
        };

        // If we are appending to the end
        if first_idx >= self.entries.len() {
            self.entries.push(Interval { start, end, value });
            return;
        }

        // Check for overlaps starting from first_idx
        let mut i = first_idx;
        let mut to_remove = 0;
        let mut prefix = None;
        let mut suffix = None;

        while i < self.entries.len() {
            let entry = self.entries.borrow(token, i);
            if entry.start >= end {
                break;
            }

            // Overlap detected
            if entry.start < start {
                // Existing interval starts before new one: [entry.start ... start ... ]
                prefix = Some(Interval {
                    start: entry.start,
                    end: start,
                    value: entry.value.clone(),
                });
            }

            if entry.end > end {
                // Existing interval ends after new one: [ ... end ... entry.end]
                suffix = Some(Interval {
                    start: end,
                    end: entry.end,
                    value: entry.value.clone(),
                });
            }

            to_remove += 1;
            i += 1;
        }

        // Apply changes
        // We might have a prefix to insert before, and a suffix to insert after.
        // We remove `to_remove` elements starting at `first_idx`

        // Optimize: verify if we can just update in place
        if to_remove == 1 && prefix.is_none() && suffix.is_none() {
             // Perfect overlap replacement
             *self.entries.borrow_mut(token, first_idx) = Interval { start, end, value };
             return;
        }

        // Generic approach: remove then insert
        for _ in 0..to_remove {
            self.entries.remove(first_idx);
        }

        // Insert new parts
        // Order: Prefix (if any), New, Suffix (if any)
        // Since we removed, we insert at `first_idx`

        let mut current_idx = first_idx;

        if let Some(p) = prefix {
            self.entries.insert(current_idx, p);
            current_idx += 1;
        }

        self.entries.insert(current_idx, Interval { start, end, value });
        current_idx += 1;

        if let Some(s) = suffix {
            self.entries.insert(current_idx, s);
        }
    }

    /// Gets the value at the given point.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, point: K) -> Option<&'a V> {
        let slice = self.entries.as_slice(token);

        match slice.binary_search_by(|entry| {
            if entry.end <= point {
                Ordering::Less
            } else if entry.start > point {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }) {
            Ok(idx) => Some(&slice[idx].value),
            Err(_) => None,
        }
    }

    /// Iterates over all intervals.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a Interval<K, V>> + 'a {
        self.entries.iter(token)
    }

    /// Iterates over intervals overlapping the given range `[start, end)`.
    pub fn iter_overlaps<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        start: K,
        end: K
    ) -> impl Iterator<Item = &'a Interval<K, V>> + 'a {
        // Zero-copy slicing of the iterator
        let slice = self.entries.as_slice(token);

        // Find start index
        let start_idx = match slice.binary_search_by(|entry| {
             if entry.end <= start {
                 Ordering::Less
             } else if entry.start >= end {
                 Ordering::Greater
             } else {
                 Ordering::Equal
             }
        }) {
            Ok(idx) => {
                let mut i = idx;
                while i > 0 && slice[i-1].end > start {
                    i -= 1;
                }
                i
            },
            Err(idx) => idx,
        };

        self.entries.iter(token)
            .skip(start_idx)
            .take_while(move |entry| entry.start < end)
    }
}

impl<'brand, K, V> Default for BrandedIntervalMap<'brand, K, V>
where
    K: Ord + Copy + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, K, V> BrandedCollection<'brand> for BrandedIntervalMap<'brand, K, V> {
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_basic_operations() {
        GhostToken::new(|mut token| {
            let mut map = BrandedIntervalMap::new();

            // [0, 10) -> "A"
            map.insert(&mut token, 0, 10, "A");
            assert_eq!(map.len(), 1);

            assert_eq!(*map.get(&token, 5).unwrap(), "A");
            assert!(map.get(&token, 10).is_none());

            // [20, 30) -> "B"
            map.insert(&mut token, 20, 30, "B");
            assert_eq!(map.len(), 2);
            assert_eq!(*map.get(&token, 25).unwrap(), "B");
        });
    }

    #[test]
    fn test_overlap_overwrite() {
        GhostToken::new(|mut token| {
            let mut map = BrandedIntervalMap::new();

            // [0, 100) -> 1
            map.insert(&mut token, 0, 100, 1);

            // [40, 60) -> 2 (should split existing)
            map.insert(&mut token, 40, 60, 2);

            // Expected: [0, 40)->1, [40, 60)->2, [60, 100)->1
            assert_eq!(map.len(), 3);

            assert_eq!(*map.get(&token, 20).unwrap(), 1);
            assert_eq!(*map.get(&token, 50).unwrap(), 2);
            assert_eq!(*map.get(&token, 80).unwrap(), 1);
        });
    }

    #[test]
    fn test_iter_overlaps() {
        GhostToken::new(|mut token| {
            let mut map = BrandedIntervalMap::new();
            map.insert(&mut token, 0, 10, 1);
            map.insert(&mut token, 20, 30, 2);
            map.insert(&mut token, 40, 50, 3);

            let overlaps: Vec<_> = map.iter_overlaps(&token, 5, 45).collect();
            assert_eq!(overlaps.len(), 3);
            assert_eq!(overlaps[0].value, 1);
            assert_eq!(overlaps[1].value, 2);
            assert_eq!(overlaps[2].value, 3);

            let overlaps2: Vec<_> = map.iter_overlaps(&token, 25, 35).collect();
            assert_eq!(overlaps2.len(), 1);
            assert_eq!(overlaps2[0].value, 2);
        });
    }
}
