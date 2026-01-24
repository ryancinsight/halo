//! `BrandedSlab` — a token-gated slab allocator.
//!
//! Implements a slab allocator where memory blocks are managed in pages.
//! Access is protected by `GhostToken`, ensuring exclusive access without locks,
//! or concurrent access via `GhostAlloc`.

use crate::{GhostToken, GhostCell};
use crate::alloc::{GhostAlloc, AllocError};
use crate::concurrency::CachePadded;
use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc};
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::thread;

// Constants
const PAGE_SIZE: usize = 4096;
const MAX_SMALL_SIZE: usize = 2048; // Anything larger goes to global allocator

// Sharding constants for Thread-Local Caching (TLH)
const SHARD_COUNT: usize = 32;
const SHARD_MASK: usize = SHARD_COUNT - 1;

thread_local! {
    /// Caches the shard index for the current thread to avoid re-hashing.
    static THREAD_SHARD_INDEX: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Helper to get the current thread's shard index.
#[inline(always)]
fn current_shard_index() -> usize {
    THREAD_SHARD_INDEX.with(|idx| {
        if let Some(i) = idx.get() {
            i
        } else {
            let mut hasher = DefaultHasher::new();
            thread::current().id().hash(&mut hasher);
            let i = (hasher.finish() as usize) & SHARD_MASK;
            idx.set(Some(i));
            i
        }
    })
}

// Tag constants for ABA prevention in free list
const TAG_SHIFT: usize = 32;
const INDEX_MASK: usize = (1 << TAG_SHIFT) - 1;
const NONE: usize = INDEX_MASK;

/// A memory page containing blocks of a specific size.
///
/// This struct is embedded at the START of the allocated 4KB page.
#[repr(C)]
struct Page {
    next: AtomicUsize, // Linked list of pages
    block_size: usize,
    free_head: AtomicUsize, // Index of the first free block (relative to first block)
    capacity: usize,
    allocated_count: AtomicUsize,
    shard_index: usize,
    // Padding to ensure the header size is a multiple of cache line (128 bytes)
    // to prevent false sharing between the header (metadata) and the first block (user data).
    // Current size: 8 + 8 + 8 + 8 + 8 + 8 = 48 bytes (on 64-bit).
    // We need 80 bytes of padding.
    _padding: [u8; 80],
}

impl Page {
    /// Allocates a new 4KB page and initializes it as a Page with blocks of `block_size`.
    fn new(block_size: usize, next_ptr: usize, shard_index: usize) -> Option<NonNull<Page>> {
        // Ensure alignment is 4KB so we can find the header via masking
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            let page_ptr = ptr as *mut Page;

            // Calculate where blocks start
            let header_size = std::mem::size_of::<Page>();
            let mut start_offset = header_size;

            let align_mask = block_size - 1;
            if (start_offset & align_mask) != 0 {
                start_offset = (start_offset + align_mask) & !align_mask;
            }

            if start_offset >= PAGE_SIZE {
                dealloc(ptr, layout);
                return None;
            }

            let available_bytes = PAGE_SIZE - start_offset;
            let capacity = available_bytes / block_size;

            if capacity == 0 {
                dealloc(ptr, layout);
                return None;
            }

            // Initialize free list
            // Blocks are indexed 0..capacity.
            // We write the index of the next free block into the block memory itself (as u32).
            let base_ptr = ptr.add(start_offset);
            for i in 0..capacity - 1 {
                let block_ptr = base_ptr.add(i * block_size);
                *(block_ptr as *mut u32) = (i + 1) as u32;
            }
            let last_block_ptr = base_ptr.add((capacity - 1) * block_size);
            *(last_block_ptr as *mut u32) = NONE as u32;

            // Write the header
            // Note: free_head starts at 0 with tag 0.
            std::ptr::write(page_ptr, Page {
                next: AtomicUsize::new(next_ptr),
                block_size,
                free_head: AtomicUsize::new(0),
                capacity,
                allocated_count: AtomicUsize::new(0),
                shard_index,
                _padding: [0; 80],
            });

            Some(NonNull::new_unchecked(page_ptr))
        }
    }

    /// Allocates a block (Concurrent/Atomic path).
    fn alloc_atomic(&self) -> Option<NonNull<u8>> {
        let mut current = self.free_head.load(Ordering::Acquire);
        loop {
            let (idx, tag) = Self::unpack(current);
            if idx == NONE {
                return None;
            }

            unsafe {
                let block_ptr = self.get_block_ptr(idx);
                let next_idx = *(block_ptr as *const u32) as usize;

                let new_tag = tag.wrapping_add(1);
                let new_head = Self::pack(next_idx, new_tag);

                match self.free_head.compare_exchange_weak(
                    current,
                    new_head,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Some(NonNull::new_unchecked(block_ptr)),
                    Err(actual) => current = actual,
                }
            }
        }
    }

    /// Allocates a block (Exclusive/Mutable path).
    /// Safe because we have exclusive access to the Page (via &mut Page).
    fn alloc_mut(&mut self) -> Option<NonNull<u8>> {
        let current = *self.free_head.get_mut();
        let (idx, tag) = Self::unpack(current);

        if idx == NONE {
            return None;
        }

        unsafe {
            let block_ptr = self.get_block_ptr(idx);
            let next_idx = *(block_ptr as *const u32) as usize;

            let new_tag = tag.wrapping_add(1);
            let new_head = Self::pack(next_idx, new_tag);

            *self.free_head.get_mut() = new_head;
            Some(NonNull::new_unchecked(block_ptr))
        }
    }

    /// Deallocates a block (Concurrent/Atomic path).
    unsafe fn dealloc_atomic(&self, ptr: NonNull<u8>) {
        let idx = self.get_block_index(ptr);
        let block_ptr = ptr.as_ptr();

        let mut current = self.free_head.load(Ordering::Acquire);
        loop {
            let (curr_head_idx, tag) = Self::unpack(current);
            let new_tag = tag.wrapping_add(1);
            let new_head = Self::pack(idx, new_tag);

            // Link this block to current head
            *(block_ptr as *mut u32) = curr_head_idx as u32;

            match self.free_head.compare_exchange_weak(
                current,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    /// Deallocates a block (Exclusive/Mutable path).
    unsafe fn dealloc_mut(&mut self, ptr: NonNull<u8>) {
        let idx = self.get_block_index(ptr);
        let block_ptr = ptr.as_ptr();

        let current = *self.free_head.get_mut();
        let (curr_head_idx, tag) = Self::unpack(current);

        let new_tag = tag.wrapping_add(1);
        let new_head = Self::pack(idx, new_tag);

        *(block_ptr as *mut u32) = curr_head_idx as u32;
        *self.free_head.get_mut() = new_head;
    }

    // --- Helpers ---

    unsafe fn get_block_ptr(&self, idx: usize) -> *mut u8 {
        let page_addr = self as *const Page as usize;
        let header_size = std::mem::size_of::<Page>();
        let align_mask = self.block_size - 1;
        let start_offset = (header_size + align_mask) & !align_mask;
        let block_offset = start_offset + idx * self.block_size;
        (page_addr + block_offset) as *mut u8
    }

    unsafe fn get_block_index(&self, ptr: NonNull<u8>) -> usize {
        let page_addr = self as *const Page as usize;
        let ptr_addr = ptr.as_ptr() as usize;
        let header_size = std::mem::size_of::<Page>();
        let align_mask = self.block_size - 1;
        let start_offset = (header_size + align_mask) & !align_mask;
        let offset = ptr_addr - page_addr - start_offset;
        offset / self.block_size
    }

    fn unpack(val: usize) -> (usize, usize) {
        (val & INDEX_MASK, val >> TAG_SHIFT)
    }

    fn pack(index: usize, tag: usize) -> usize {
        (tag << TAG_SHIFT) | (index & INDEX_MASK)
    }

    unsafe fn from_ptr(ptr: NonNull<u8>) -> NonNull<Page> {
        let addr = ptr.as_ptr() as usize;
        let page_addr = addr & !(PAGE_SIZE - 1);
        NonNull::new_unchecked(page_addr as *mut Page)
    }
}

/// Internal state of the slab allocator.
struct SlabState {
    // Array of page lists, one for each size class (powers of 2, starting at 8)
    // Stored as AtomicUsize (pointers to Page) to support shared access.
    // CachePadded to prevent false sharing between heads of different size classes.
    // Sharded by thread ID to reduce contention.
    heads: [[CachePadded<AtomicUsize>; SHARD_COUNT]; 9],
}

impl SlabState {
    fn new() -> Self {
        Self {
            heads: core::array::from_fn(|_| core::array::from_fn(|_| CachePadded::new(AtomicUsize::new(0)))),
        }
    }

    fn get_class_index(size: usize) -> Option<usize> {
        if size <= 8 { return Some(0); }
        if size > MAX_SMALL_SIZE { return None; }

        let mut idx = 0;
        let mut s = 8;
        while s < size {
            s <<= 1;
            idx += 1;
        }
        Some(idx)
    }

    fn get_block_size(class_idx: usize) -> usize {
        8 << class_idx
    }
}

impl Drop for SlabState {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            for class_heads in &mut self.heads {
                for head_atomic in class_heads {
                    // We have exclusive access in Drop, so we can use load(Relaxed) or get_mut
                    let mut page_ptr_val = *head_atomic.get_mut();
                    while page_ptr_val != 0 {
                        let page = &mut *(page_ptr_val as *mut Page);
                        let next_val = *page.next.get_mut(); // AtomicUsize::get_mut is safe here

                        // Drop the page memory
                        dealloc(page_ptr_val as *mut u8, layout);
                        page_ptr_val = next_val;
                    }
                }
            }
        }
    }
}

/// A branded slab allocator.
///
/// # TODOs and Future Optimizations
///
/// - **TODO(perf): Thread-Local Caching (TLH)**:
///   Currently, `GhostAlloc` implementation suffers from contention on the `head` page of each size class.
///   Implementing a thread-local cache (via `ThreadLocal` or `SharedGhostToken` sharding) would
///   significantly improve throughput for high-concurrency workloads, similar to `mimalloc`'s free list sharding.
///
/// - **TODO(perf): Restartable Sequences (RSEQ)**:
///   On Linux, using RSEQ could allow faster per-cpu operations without atomics for the fast path.
///
/// - **TODO(mem): Eager Page Return**:
///   Currently, empty pages are only returned when the Slab is dropped. Implementing logic to
///   return pages to the OS (or a global pool) when they become empty would reduce memory footprint.
pub struct BrandedSlab<'brand> {
    state: GhostCell<'brand, SlabState>,
}

impl<'brand> BrandedSlab<'brand> {
    /// Creates a new branded slab allocator.
    pub fn new() -> Self {
        Self {
            state: GhostCell::new(SlabState::new()),
        }
    }

    /// Manually triggers reclamation of empty pages.
    /// This is useful when mostly using concurrent allocation/deallocation, where pages are not automatically returned to OS.
    pub fn compact(&self, token: &mut GhostToken<'brand>) {
        let state = self.state.borrow_mut(token);
        unsafe {
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            for class_heads in &mut state.heads {
                for head_atomic in class_heads {
                    let mut current_ptr_val = *head_atomic.get_mut();
                    let mut prev_ptr_val = 0;

                    while current_ptr_val != 0 {
                        let page = &mut *(current_ptr_val as *mut Page);
                        let next_val = *page.next.get_mut();

                        if *page.allocated_count.get_mut() == 0 {
                            // Unlink
                            if prev_ptr_val == 0 {
                                *head_atomic.get_mut() = next_val;
                            } else {
                                let prev_page = &mut *(prev_ptr_val as *mut Page);
                                *prev_page.next.get_mut() = next_val;
                            }

                            // Free
                            dealloc(current_ptr_val as *mut u8, layout);

                            // Move to next
                            current_ptr_val = next_val;
                        } else {
                            prev_ptr_val = current_ptr_val;
                            current_ptr_val = next_val;
                        }
                    }
                }
            }
        }
    }

    /// Allocates memory with exclusive access.
    ///
    /// This method is faster than `GhostAlloc::allocate` as it avoids atomic operations.
    pub fn allocate_mut(
        &self,
        token: &mut GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());
        let state = self.state.borrow_mut(token);

        if let Some(class_idx) = SlabState::get_class_index(size) {
            let block_size = SlabState::get_block_size(class_idx);
            let shard_idx = current_shard_index();
            let head_atomic = &mut state.heads[class_idx][shard_idx];

            // Try head page
            let mut page_ptr_val = *head_atomic.get_mut();
            if page_ptr_val != 0 {
                unsafe {
                    let page = &mut *(page_ptr_val as *mut Page);
                    if let Some(ptr) = page.alloc_mut() {
                        *page.allocated_count.get_mut() += 1;
                        return Ok(ptr);
                    }
                }
            }

            // Head full or empty, allocate new page
            // Optimization: We push new page to front
            if let Some(new_page_ptr) = Page::new(block_size, page_ptr_val, shard_idx) {
                unsafe {
                    let mut new_page = new_page_ptr;
                    let ptr = new_page.as_mut().alloc_mut().ok_or(AllocError)?;
                    *new_page.as_mut().allocated_count.get_mut() += 1;

                    *head_atomic.get_mut() = new_page_ptr.as_ptr() as usize;

                    Ok(ptr)
                }
            } else {
                Err(AllocError)
            }
        } else {
            // Large allocation
            unsafe {
                let ptr = alloc(layout);
                NonNull::new(ptr).ok_or(AllocError)
            }
        }
    }

    /// Deallocates memory with exclusive access.
    pub unsafe fn deallocate_mut(
        &self,
        token: &mut GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if let Some(class_idx) = SlabState::get_class_index(size) {
            let mut page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_mut();
            page.dealloc_mut(ptr);
            *page.allocated_count.get_mut() -= 1;
            if *page.allocated_count.get_mut() == 0 {
                self.unlink_and_free(token, page_ptr, page.shard_index, class_idx);
            }
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }

    unsafe fn unlink_and_free(
        &self,
        token: &mut GhostToken<'brand>,
        page_ptr: NonNull<Page>,
        shard_idx: usize,
        class_idx: usize,
    ) {
        let state = self.state.borrow_mut(token);
        let head_atomic = &mut state.heads[class_idx][shard_idx];

        let mut current_ptr_val = *head_atomic.get_mut();
        let target_ptr_val = page_ptr.as_ptr() as usize;

        if current_ptr_val == target_ptr_val {
            // It's the head
            let page = &mut *(current_ptr_val as *mut Page);
            *head_atomic.get_mut() = *page.next.get_mut();

            // Free the page
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            dealloc(page_ptr.as_ptr() as *mut u8, layout);
            return;
        }

        while current_ptr_val != 0 {
            let page = &mut *(current_ptr_val as *mut Page);
            let next_val = *page.next.get_mut();

            if next_val == target_ptr_val {
                // Found predecessor
                let target_page = &mut *(target_ptr_val as *mut Page);
                *page.next.get_mut() = *target_page.next.get_mut();

                // Free the page
                let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
                dealloc(page_ptr.as_ptr() as *mut u8, layout);
                return;
            }

            current_ptr_val = next_val;
        }
    }
}

impl<'brand> Default for BrandedSlab<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

// --- Concurrent Access Implementation ---
// This is now the default GhostAlloc implementation.
impl<'brand> GhostAlloc<'brand> for BrandedSlab<'brand> {
    fn allocate(
        &self,
        token: &GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());
        let state = self.state.borrow(token);

        if let Some(class_idx) = SlabState::get_class_index(size) {
             let shard_idx = current_shard_index();
             let head_atomic = &state.heads[class_idx][shard_idx];

            // 1. Try to allocate from existing pages
            let mut page_ptr_val = head_atomic.load(Ordering::Acquire);
            while page_ptr_val != 0 {
                unsafe {
                    let page = &*(page_ptr_val as *const Page);
                    if let Some(ptr) = page.alloc_atomic() {
                        page.allocated_count.fetch_add(1, Ordering::Relaxed);
                        return Ok(ptr);
                    }
                    page_ptr_val = page.next.load(Ordering::Acquire);
                }
            }

            // 2. No space found, allocate new page
            let block_size = SlabState::get_block_size(class_idx);

            loop {
                let current_head = head_atomic.load(Ordering::Acquire);
                if let Some(new_page) = Page::new(block_size, current_head, shard_idx) {
                    let new_page_val = new_page.as_ptr() as usize;

                    match head_atomic.compare_exchange(
                        current_head,
                        new_page_val,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            unsafe {
                                let page = new_page.as_ref();
                                let ptr = page.alloc_atomic().ok_or(AllocError)?;
                                page.allocated_count.fetch_add(1, Ordering::Relaxed);
                                return Ok(ptr);
                            }
                        }
                        Err(_) => {
                            unsafe {
                                let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
                                dealloc(new_page.as_ptr() as *mut u8, layout);
                            }
                        }
                    }
                } else {
                    return Err(AllocError);
                }
            }
        } else {
            unsafe {
                let ptr = alloc(layout);
                NonNull::new(ptr).ok_or(AllocError)
            }
        }
    }

    unsafe fn deallocate(
        &self,
        _token: &GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if SlabState::get_class_index(size).is_some() {
            let page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_ref();
            page.dealloc_atomic(ptr);
            page.allocated_count.fetch_sub(1, Ordering::Relaxed);
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GhostToken, SharedGhostToken};
    use std::thread;

    #[test]
    fn test_branded_slab_basic() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();

            // Concurrent alloc with exclusive token reborrow
            let ptr1 = slab.allocate(&token, layout).unwrap();
            let ptr2 = slab.allocate(&token, layout).unwrap();

            unsafe {
                *(ptr1.as_ptr() as *mut u64) = 123;
                *(ptr2.as_ptr() as *mut u64) = 456;
                assert_eq!(*(ptr1.as_ptr() as *mut u64), 123);
                assert_eq!(*(ptr2.as_ptr() as *mut u64), 456);

                slab.deallocate(&token, ptr1, layout);
                slab.deallocate(&token, ptr2, layout);
            }

            // Exclusive alloc (optimized)
            let ptr3 = slab.allocate_mut(&mut token, layout).unwrap();
            unsafe {
                *(ptr3.as_ptr() as *mut u64) = 789;
                slab.deallocate_mut(&mut token, ptr3, layout);
            }
        });
    }

    #[test]
    fn test_concurrent_access() {
        GhostToken::new(|token| {
            let slab = BrandedSlab::new();
            let shared_token = SharedGhostToken::new(token);
            let slab_ref = &slab;
            let token_ref = &shared_token;

            thread::scope(|s| {
                for t_idx in 0..4 {
                    s.spawn(move || {
                        let guard = token_ref.read();
                        let layout = Layout::new::<u64>();
                        for i in 0..100 {
                            let ptr = slab_ref.allocate(&guard, layout).unwrap();
                            unsafe {
                                *(ptr.as_ptr() as *mut u64) = (t_idx * 1000 + i) as u64;
                                slab_ref.deallocate(&guard, ptr, layout);
                            }
                        }
                    });
                }
            });
        });
    }

    #[test]
    fn test_eager_page_return() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();

            // Allocate enough to fill a page and start a new one
            // Page size 4096. Block size 8 (min).
            // Page capacity ~ (4096 - 48 header - 80 padding) / 8 = 3968 / 8 = 496.

            let mut ptrs = Vec::new();
            for _ in 0..600 {
                ptrs.push(slab.allocate_mut(&mut token, layout).unwrap());
            }

            // Now free them all
            for ptr in ptrs {
                unsafe {
                    slab.deallocate_mut(&mut token, ptr, layout);
                }
            }

            // If eager return works, pages should be freed.
            // We can't easily assert on memory usage here, but we verify no crash/corruption.

            // Allocate again to see if it works
            let ptr = slab.allocate_mut(&mut token, layout).unwrap();
            unsafe { slab.deallocate_mut(&mut token, ptr, layout); }
        });
    }

    #[test]
    fn test_compact() {
        GhostToken::new(|token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();
            let shared_token = SharedGhostToken::new(token);
            let slab_ref = &slab;
            let token_ref = &shared_token;

            // Use concurrent allocation/deallocation to leave empty pages
            thread::scope(|s| {
                for _ in 0..4 {
                    s.spawn(move || {
                        let guard = token_ref.read();
                        let mut ptrs = Vec::new();
                        for _ in 0..100 {
                            ptrs.push(slab_ref.allocate(&guard, layout).unwrap());
                        }
                        for ptr in ptrs {
                            unsafe { slab_ref.deallocate(&guard, ptr, layout); }
                        }
                    });
                }
            });

            // Now we have empty pages (likely).
            // Reclaim them.
            // Need mut token.
            let mut guard = shared_token.write();
            let token = &mut *guard;
            slab.compact(token);

            // Verify we can still allocate
            let ptr = slab.allocate_mut(token, layout).unwrap();
            unsafe { slab.deallocate_mut(token, ptr, layout); }
        });
    }
}
