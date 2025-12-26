//! Branded atomic bitsets.
//!
//! This is a dense alternative to `Vec<AtomicBool>` for visited sets / flags.

use core::sync::atomic::Ordering;

use super::GhostAtomicUsize;

/// A branded, word-packed atomic bitset.
pub struct GhostAtomicBitset<'brand> {
    bits: usize,
    words: Vec<GhostAtomicUsize<'brand>>,
}

impl<'brand> GhostAtomicBitset<'brand> {
    /// Creates a new bitset with `bits` bits, all cleared.
    pub fn new(bits: usize) -> Self {
        let word_bits = usize::BITS as usize;
        let words_len = bits.div_ceil(word_bits);
        let words = (0..words_len).map(|_| GhostAtomicUsize::new(0)).collect();
        Self { bits, words }
    }

    /// Number of bits.
    pub fn len_bits(&self) -> usize {
        self.bits
    }

    /// Clears all bits.
    pub fn clear_all(&self) {
        for w in &self.words {
            w.store(0, Ordering::Relaxed);
        }
    }

    /// Returns whether `bit` is set.
    ///
    /// # Panics
    /// Panics if `bit >= len_bits()`.
    pub fn is_set(&self, bit: usize) -> bool {
        assert!(bit < self.bits);
        // SAFETY: index checked above.
        unsafe { self.is_set_unchecked(bit) }
    }

    /// Sets `bit` and returns `true` iff this call observed it previously cleared.
    ///
    /// # Panics
    /// Panics if `bit >= len_bits()`.
    pub fn test_and_set(&self, bit: usize, order: Ordering) -> bool {
        assert!(bit < self.bits);
        // SAFETY: index checked above.
        unsafe { self.test_and_set_unchecked(bit, order) }
    }

    /// # Safety
    /// Caller must ensure `bit < len_bits()`.
    #[inline(always)]
    pub unsafe fn is_set_unchecked(&self, bit: usize) -> bool {
        let (word, mask) = bit_word_mask(bit);
        // SAFETY: word index derived from bit < self.bits.
        (self.words.get_unchecked(word).load(Ordering::Relaxed) & mask) != 0
    }

    /// # Safety
    /// Caller must ensure `bit < len_bits()`.
    #[inline(always)]
    pub unsafe fn test_and_set_unchecked(&self, bit: usize, order: Ordering) -> bool {
        let (word, mask) = bit_word_mask(bit);
        // SAFETY: word index derived from bit < self.bits.
        let prev = self.fetch_or_word_unchecked(word, mask, order);
        (prev & mask) == 0
    }

    /// # Safety
    /// Caller must ensure `word < self.words.len()`.
    #[inline(always)]
    pub(crate) unsafe fn fetch_or_word_unchecked(
        &self,
        word: usize,
        mask: usize,
        order: Ordering,
    ) -> usize {
        // SAFETY: caller guarantees word index is valid.
        self.words.get_unchecked(word).fetch_or(mask, order)
    }
}

#[inline(always)]
fn bit_word_mask(bit: usize) -> (usize, usize) {
    // `usize::BITS` is always a power-of-two (32 or 64), so use shifts/masks.
    // This is on the hot path for graph traversal.
    #[cfg(target_pointer_width = "64")]
    {
        let word = bit >> 6;
        let shift = bit & 63;
        return (word, 1usize << shift);
    }
    #[cfg(target_pointer_width = "32")]
    {
        let word = bit >> 5;
        let shift = bit & 31;
        return (word, 1usize << shift);
    }
}


