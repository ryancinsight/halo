//! `BrandedBloomFilter` — a probabilistic data structure with token-gated access.
//!
//! Uses `BrandedBitSet` to store bits. Supports `insert` and `contains`.
//! Uses double hashing to simulate `k` hash functions.

use crate::collections::other::bit_set::BrandedBitSet;
use crate::GhostToken;
use core::hash::{BuildHasher, Hash, Hasher};
use std::collections::hash_map::RandomState;
use std::marker::PhantomData;

/// A branded Bloom filter.
pub struct BrandedBloomFilter<'brand, T, S = RandomState> {
    bits: BrandedBitSet<'brand>,
    /// Number of hash functions (k).
    num_hashes: u32,
    /// Size of the bit array (m).
    bit_size: usize,
    hasher: S,
    _marker: PhantomData<T>,
}

impl<'brand, T> BrandedBloomFilter<'brand, T> {
    /// Creates a new Bloom filter with default parameters (optimized for 100 items, 1% false positive).
    pub fn new() -> Self {
        Self::with_capacity_and_fp_rate(100, 0.01)
    }

    /// Creates a new Bloom filter optimized for `expected_items` and `fp_rate`.
    pub fn with_capacity_and_fp_rate(expected_items: usize, fp_rate: f64) -> Self {
        Self::with_capacity_fp_rate_and_hasher(expected_items, fp_rate, RandomState::new())
    }
}

impl<'brand, T, S> BrandedBloomFilter<'brand, T, S> {
    /// Creates a new Bloom filter with custom hasher.
    pub fn with_capacity_fp_rate_and_hasher(
        expected_items: usize,
        fp_rate: f64,
        hasher: S,
    ) -> Self {
        // m = - (n * ln p) / (ln 2)^2
        let n = expected_items as f64;
        let ln2 = std::f64::consts::LN_2;
        let m = -1.0 * (n * fp_rate.ln()) / (ln2 * ln2);
        let bit_size = m.ceil() as usize;

        // k = (m / n) * ln 2
        let k = (m / n) * ln2;
        let num_hashes = k.ceil() as u32;

        Self {
            bits: BrandedBitSet::with_capacity(bit_size),
            num_hashes,
            bit_size,
            hasher,
            _marker: PhantomData,
        }
    }

    /// Clears the Bloom filter.
    pub fn clear(&mut self) {
        self.bits.clear();
    }

    /// Returns the number of bits set (approximate load).
    pub fn set_bits_count(&self) -> usize {
        self.bits.len()
    }
}

impl<'brand, T, S> BrandedBloomFilter<'brand, T, S>
where
    T: Hash,
    S: BuildHasher,
{
    /// Helper to compute two hashes.
    fn get_hashes(&self, item: &T) -> (u64, u64) {
        let mut hasher = self.hasher.build_hasher();
        item.hash(&mut hasher);
        let h1 = hasher.finish();

        // Use a mixing strategy to generate a second hash h2 from h1.
        // This avoids traversing the item a second time.
        // The mixing constants are from MurmurHash3's 64-bit finalizer.
        let mut h2 = h1;
        h2 = (h2 ^ (h2 >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
        h2 = (h2 ^ (h2 >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        h2 = h2 ^ (h2 >> 33);

        (h1, h2)
    }

    /// Adds an item to the Bloom filter.
    pub fn insert(&mut self, token: &mut GhostToken<'brand>, item: &T) {
        let (h1, h2) = self.get_hashes(item);
        let m = self.bit_size as u64;

        for i in 0..self.num_hashes {
            let idx = (h1.wrapping_add((i as u64).wrapping_mul(h2))) % m;
            self.bits.insert(token, idx as usize);
        }
    }

    /// Checks if an item is possibly in the Bloom filter.
    pub fn contains(&self, token: &GhostToken<'brand>, item: &T) -> bool {
        let (h1, h2) = self.get_hashes(item);
        let m = self.bit_size as u64;

        for i in 0..self.num_hashes {
            let idx = (h1.wrapping_add((i as u64).wrapping_mul(h2))) % m;
            if !self.bits.contains(token, idx as usize) {
                return false;
            }
        }
        true
    }

}

impl<'brand, T> Default for BrandedBloomFilter<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_bloom_basic() {
        GhostToken::new(|mut token| {
            let mut bloom = BrandedBloomFilter::with_capacity_and_fp_rate(100, 0.01);

            bloom.insert(&mut token, &"hello");
            bloom.insert(&mut token, &"world");

            assert!(bloom.contains(&token, &"hello"));
            assert!(bloom.contains(&token, &"world"));
            assert!(!bloom.contains(&token, &"foo"));
        });
    }

    #[test]
    fn test_bloom_fp_rate() {
        // Probabilistic test: verify that false positive rate is reasonable.
        // With 1000 items and 1% FP rate.
        GhostToken::new(|mut token| {
            let mut bloom = BrandedBloomFilter::with_capacity_and_fp_rate(1000, 0.01);

            for i in 0..1000 {
                bloom.insert(&mut token, &i);
            }

            // All inserted items must be found (no false negatives)
            for i in 0..1000 {
                assert!(bloom.contains(&token, &i));
            }

            // Check false positives on non-inserted items
            let mut fp_count = 0;
            let trials = 10000;
            for i in 1000..(1000 + trials) {
                if bloom.contains(&token, &i) {
                    fp_count += 1;
                }
            }

            let measured_rate = fp_count as f64 / trials as f64;
            // Should be around 0.01. Allow some variance.
            assert!(measured_rate < 0.05, "FP rate too high: {}", measured_rate);
        });
    }
}
