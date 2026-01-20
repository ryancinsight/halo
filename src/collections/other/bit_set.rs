//! `BrandedBitSet` — a high-performance bit set with token-gated access.
//!
//! This implementation uses `BrandedVec<u64>` to store bits, allowing for efficient
//! space usage and fast set operations (union, intersection, etc.) using bitwise SIMD-compatible logic.
//!
//! Access is controlled via `GhostToken`.

use crate::collections::vec::BrandedVec;
use crate::GhostToken;
use std::cmp;

/// A branded bit set.
pub struct BrandedBitSet<'brand> {
    words: BrandedVec<'brand, u64>,
    /// Number of set bits.
    len: usize,
}

impl<'brand> BrandedBitSet<'brand> {
    /// Creates a new empty bit set.
    pub fn new() -> Self {
        Self {
            words: BrandedVec::new(),
            len: 0,
        }
    }

    /// Creates a new bit set with initial capacity (in bits).
    pub fn with_capacity(bits: usize) -> Self {
        let words = (bits + 63) / 64;
        Self {
            words: BrandedVec::with_capacity(words),
            len: 0,
        }
    }

    /// Returns the number of set bits.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clears the bit set.
    pub fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    /// Adds a value to the set. Returns `true` if the value was not already present.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, bit: usize) -> bool {
        let word_idx = bit / 64;
        let bit_idx = bit % 64;
        let mask = 1u64 << bit_idx;

        // Ensure capacity
        if word_idx >= self.words.len() {
            let additional_words = word_idx - self.words.len() + 1;
            self.words.reserve(additional_words);
            for _ in 0..additional_words {
                self.words.push(0);
            }
        }

        let word = self.words.borrow_mut(token, word_idx);
        if *word & mask == 0 {
            *word |= mask;
            self.len += 1;
            true
        } else {
            false
        }
    }

    /// Removes a value from the set. Returns `true` if the value was present.
    pub fn remove(&mut self, token: &mut GhostToken<'brand>, bit: usize) -> bool {
        let word_idx = bit / 64;
        if word_idx >= self.words.len() {
            return false;
        }

        let bit_idx = bit % 64;
        let mask = 1u64 << bit_idx;
        let word = self.words.borrow_mut(token, word_idx);

        if *word & mask != 0 {
            *word &= !mask;
            self.len -= 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if the set contains the value.
    pub fn contains(&self, token: &GhostToken<'brand>, bit: usize) -> bool {
        let word_idx = bit / 64;
        if word_idx >= self.words.len() {
            return false;
        }

        let bit_idx = bit % 64;
        let mask = 1u64 << bit_idx;
        let word = self.words.borrow(token, word_idx);
        (*word & mask) != 0
    }

    /// Reserves capacity for at least `additional` bits.
    pub fn reserve(&mut self, additional: usize) {
        // We want to reserve space for `additional` more bits.
        // Each word holds 64 bits.
        let additional_words = (additional + 63) / 64;
        self.words.reserve(additional_words);
    }

    // --- Set Operations ---

    /// Unions with another bit set: `self |= other`.
    pub fn union_with(&mut self, token: &mut GhostToken<'brand>, other: &BrandedBitSet<'brand>) {
        let other_len = other.words.len();
        // Ensure self is large enough
        while self.words.len() < other_len {
            self.words.push(0);
        }

        // Use direct slice access for performance
        // We use as_mut_slice_exclusive so we don't lock the token mutably,
        // allowing us to use the token to read 'other'.
        let self_slice = self.words.as_mut_slice_exclusive();
        let other_slice = other.words.as_slice(token);

        let common_len = cmp::min(self_slice.len(), other_slice.len());

        let mut new_bits = 0;

        for i in 0..common_len {
            let old_val = self_slice[i];
            let new_val = old_val | other_slice[i];
            if old_val != new_val {
                // To keep len accurate, we'd need to count bits.
                // Recomputing len from scratch might be faster if changes are frequent,
                // or using popcount on the diff.
                // popcount(new_val) - popcount(old_val) doesn't work directly if bits were already set.
                // diff = new_val ^ old_val. All these bits were 0 and are now 1.
                let diff = new_val ^ old_val;
                new_bits += diff.count_ones() as usize;
                self_slice[i] = new_val;
            }
        }
        self.len += new_bits;
    }

    /// Intersects with another bit set: `self &= other`.
    pub fn intersect_with(&mut self, token: &mut GhostToken<'brand>, other: &BrandedBitSet<'brand>) {
        let self_len = self.words.len();
        let other_len = other.words.len();

        let self_slice = self.words.as_mut_slice_exclusive();
        let other_slice = other.words.as_slice(token);

        let common_len = cmp::min(self_len, other_len);

        let mut removed_bits = 0;

        for i in 0..common_len {
            let old_val = self_slice[i];
            let new_val = old_val & other_slice[i];
            let diff = old_val ^ new_val; // bits that turned 0
            removed_bits += diff.count_ones() as usize;
            self_slice[i] = new_val;
        }

        // Clear remaining words in self
        if self_len > other_len {
            for i in other_len..self_len {
                let old_val = self_slice[i];
                removed_bits += old_val.count_ones() as usize;
                self_slice[i] = 0;
            }
        }

        self.len -= removed_bits;
    }

    /// Differences with another bit set: `self &= !other`.
    pub fn difference_with(&mut self, token: &mut GhostToken<'brand>, other: &BrandedBitSet<'brand>) {
        let self_len = self.words.len();
        let other_len = other.words.len();

        let self_slice = self.words.as_mut_slice_exclusive();
        let other_slice = other.words.as_slice(token);

        let common_len = cmp::min(self_len, other_len);
        let mut removed_bits = 0;

        for i in 0..common_len {
            let old_val = self_slice[i];
            let new_val = old_val & !other_slice[i];
            let diff = old_val ^ new_val;
            removed_bits += diff.count_ones() as usize;
            self_slice[i] = new_val;
        }

        self.len -= removed_bits;
    }

    /// Symmetric difference with another bit set: `self ^= other`.
    pub fn symmetric_difference_with(&mut self, token: &mut GhostToken<'brand>, other: &BrandedBitSet<'brand>) {
        let other_len = other.words.len();
        while self.words.len() < other_len {
            self.words.push(0);
        }

        let self_slice = self.words.as_mut_slice_exclusive();
        let other_slice = other.words.as_slice(token);
        let common_len = cmp::min(self_slice.len(), other_slice.len());

        // For sym diff, we can't easily track len incrementally without popcount on result.
        // Or: len = len(self) + len(other) - 2 * len(intersection)
        // Let's recompute len or track it carefully.
        // bit_diff = popcount(new) - popcount(old)

        let mut len_delta: isize = 0;

        for i in 0..common_len {
            let old_val = self_slice[i];
            let new_val = old_val ^ other_slice[i];

            let old_cnt = old_val.count_ones() as isize;
            let new_cnt = new_val.count_ones() as isize;
            len_delta += new_cnt - old_cnt;

            self_slice[i] = new_val;
        }

        self.len = (self.len as isize + len_delta) as usize;
    }

    // --- Iterators ---

    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> Iter<'a, 'brand> {
        Iter {
            iter: self.words.iter(token).enumerate(),
            current_word: 0,
            word_idx: 0,
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct Iter<'a, 'brand> {
    iter: std::iter::Enumerate<std::slice::Iter<'a, u64>>,
    current_word: u64,
    word_idx: usize,
    _marker: std::marker::PhantomData<&'brand ()>,
}

impl<'a, 'brand> Iterator for Iter<'a, 'brand> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_word != 0 {
                let trailing = self.current_word.trailing_zeros();
                self.current_word &= self.current_word - 1; // clear lowest bit
                return Some(self.word_idx * 64 + trailing as usize);
            }

            match self.iter.next() {
                Some((idx, &word)) => {
                    self.word_idx = idx;
                    self.current_word = word;
                }
                None => return None,
            }
        }
    }
}

impl<'brand> Default for BrandedBitSet<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_bit_set_basic() {
        GhostToken::new(|mut token| {
            let mut set = BrandedBitSet::new();
            assert!(set.is_empty());

            assert!(set.insert(&mut token, 1));
            assert!(set.insert(&mut token, 100));
            assert_eq!(set.len(), 2);
            assert!(set.contains(&token, 1));
            assert!(set.contains(&token, 100));
            assert!(!set.contains(&token, 2));

            assert!(!set.insert(&mut token, 1)); // Already present
            assert_eq!(set.len(), 2);

            assert!(set.remove(&mut token, 1));
            assert_eq!(set.len(), 1);
            assert!(!set.contains(&token, 1));
            assert!(!set.remove(&mut token, 1));
        });
    }

    #[test]
    fn test_bit_set_iter() {
        GhostToken::new(|mut token| {
            let mut set = BrandedBitSet::new();
            set.insert(&mut token, 1);
            set.insert(&mut token, 5);
            set.insert(&mut token, 64);
            set.insert(&mut token, 128);

            let collected: Vec<_> = set.iter(&token).collect();
            assert_eq!(collected, vec![1, 5, 64, 128]);
        });
    }

    #[test]
    fn test_bit_set_union() {
        GhostToken::new(|mut token| {
            let mut set1 = BrandedBitSet::new();
            set1.insert(&mut token, 1);
            set1.insert(&mut token, 2);

            let mut set2 = BrandedBitSet::new();
            set2.insert(&mut token, 2);
            set2.insert(&mut token, 3);

            set1.union_with(&mut token, &set2);
            assert_eq!(set1.len(), 3);
            assert!(set1.contains(&token, 1));
            assert!(set1.contains(&token, 2));
            assert!(set1.contains(&token, 3));
        });
    }

    #[test]
    fn test_bit_set_intersect() {
        GhostToken::new(|mut token| {
            let mut set1 = BrandedBitSet::new();
            set1.insert(&mut token, 1);
            set1.insert(&mut token, 2);

            let mut set2 = BrandedBitSet::new();
            set2.insert(&mut token, 2);
            set2.insert(&mut token, 3);

            set1.intersect_with(&mut token, &set2);
            assert_eq!(set1.len(), 1);
            assert!(set1.contains(&token, 2));
            assert!(!set1.contains(&token, 1));
        });
    }
}
