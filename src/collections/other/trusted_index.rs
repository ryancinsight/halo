use std::marker::PhantomData;
use std::num::NonZeroUsize;

/// A trusted index that is guaranteed to be valid for a specific brand.
///
/// This wrapper around `NonZeroUsize` carries a `'brand` lifetime, implying that
/// it was created by a trusted source (like a graph) and checked against bounds.
/// It enables `get_unchecked` access patterns where the index validity is invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct TrustedIndex<'brand> {
    idx: NonZeroUsize,
    _marker: PhantomData<&'brand ()>,
}

impl<'brand> TrustedIndex<'brand> {
    /// Creates a new `TrustedIndex` from a raw index.
    ///
    /// # Safety
    /// The caller must ensure that `idx` is a valid index for the branded collection.
    #[inline(always)]
    pub unsafe fn new_unchecked(idx: usize) -> Self {
        // We store 1-based index to use NonZeroUsize optimization
        Self {
            idx: NonZeroUsize::new_unchecked(idx + 1),
            _marker: PhantomData,
        }
    }

    /// Returns the raw 0-based index.
    #[inline(always)]
    pub fn get(&self) -> usize {
        self.idx.get() - 1
    }
}
