use core::sync::atomic::{AtomicUsize, Ordering};
use crate::GhostToken;
use crate::token::traits::GhostBorrow;

// Mask for 48-bit pointers (standard x86_64 userspace)
const PTR_MASK: usize = 0x0000_FFFF_FFFF_FFFF;
const TAG_SHIFT: usize = 48;

#[inline(always)]
fn pack(ptr: *mut u8, tag: usize) -> usize {
    (ptr as usize & PTR_MASK) | (tag << TAG_SHIFT)
}

#[inline(always)]
fn unpack(val: usize) -> (*mut u8, usize) {
    ((val & PTR_MASK) as *mut u8, val >> TAG_SHIFT)
}

/// A lock-free freelist for raw memory blocks.
///
/// Uses a Treiber stack with tagged pointers to prevent ABA.
/// Requires blocks to be at least `size_of::<usize>()`.
pub struct BrandedFreelist<'brand> {
    head: AtomicUsize,
    _marker: core::marker::PhantomData<fn(&GhostToken<'brand>)>,
}

impl<'brand> BrandedFreelist<'brand> {
    pub const fn new() -> Self {
        Self {
            head: AtomicUsize::new(0), // 0 is null
            _marker: core::marker::PhantomData,
        }
    }

    /// Pushes a single block onto the list.
    ///
    /// # Safety
    /// `ptr` must be valid, aligned, and point to a block of at least `usize` bytes.
    /// The block must be effectively owned by the caller (not accessible by others).
    pub unsafe fn push(&self, _token: &impl GhostBorrow<'brand>, ptr: *mut u8) {
        let mut current = self.head.load(Ordering::Relaxed);
        loop {
            let (next_ptr, tag) = unpack(current);

            // Write next pointer into the block
            *(ptr as *mut *mut u8) = next_ptr;

            let new_tag = tag.wrapping_add(1);
            let new_head = pack(ptr, new_tag);

            match self.head.compare_exchange_weak(
                current,
                new_head,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// Pushes a batch of blocks onto the list.
    ///
    /// The blocks in `iter` are linked together and pushed as a single chain.
    /// This reduces contention on the head pointer.
    /// Returns the number of blocks pushed.
    pub unsafe fn push_batch<I>(&self, _token: &impl GhostBorrow<'brand>, iter: I) -> usize
    where
        I: IntoIterator<Item = *mut u8>,
    {
        let mut iter = iter.into_iter();
        let first = match iter.next() {
            Some(p) => p,
            None => return 0,
        };

        let mut last = first;
        let mut count = 1;

        // Link the batch locally
        for ptr in iter {
            *(ptr as *mut *mut u8) = last;
            last = ptr;
            count += 1;
        }

        let local_head = last;
        let local_tail = first;

        let mut current = self.head.load(Ordering::Relaxed);
        loop {
            let (old_head_ptr, tag) = unpack(current);

            // Link tail to old head
            *(local_tail as *mut *mut u8) = old_head_ptr;

            let new_tag = tag.wrapping_add(1);
            let new_val = pack(local_head, new_tag);

            match self.head.compare_exchange_weak(
                current,
                new_val,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return count,
                Err(actual) => current = actual,
            }
        }
    }

    /// Pops a single block from the list.
    pub unsafe fn pop(&self, _token: &impl GhostBorrow<'brand>) -> Option<*mut u8> {
        let mut current = self.head.load(Ordering::Acquire);
        loop {
            let (ptr, tag) = unpack(current);
            if ptr.is_null() {
                return None;
            }

            // Read next pointer from the block
            let next_ptr = *(ptr as *mut *mut u8);

            let new_tag = tag.wrapping_add(1);
            let new_head = pack(next_ptr, new_tag);

            match self.head.compare_exchange_weak(
                current,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(ptr),
                Err(actual) => current = actual,
            }
        }
    }

    /// Pops a batch of up to `n` blocks.
    pub unsafe fn pop_batch(&self, token: &impl GhostBorrow<'brand>, n: usize) -> Vec<*mut u8> {
        let mut vec = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(ptr) = self.pop(token) {
                vec.push(ptr);
            } else {
                break;
            }
        }
        vec
    }

    /// Pops all items. Unsafe because it ignores the token.
    /// Intended for cleanup when the freelist is exclusively owned.
    pub unsafe fn pop_all(&mut self) -> Vec<*mut u8> {
        let mut vec = Vec::new();
        let mut current = self.head.load(Ordering::Relaxed);
        loop {
            let (ptr, _tag) = unpack(current);
            if ptr.is_null() {
                break;
            }
            vec.push(ptr);
            let next_ptr = *(ptr as *mut *mut u8);
            // We construct a fake packed value to continue traversal. Tag doesn't matter.
            current = pack(next_ptr, 0);
        }
        self.head.store(0, Ordering::Relaxed);
        vec
    }
}

// Safety: It's a lock-free structure.
unsafe impl<'brand> Send for BrandedFreelist<'brand> {}
unsafe impl<'brand> Sync for BrandedFreelist<'brand> {}
