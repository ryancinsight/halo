//! `ConcurrentBrandedSlab` — a lock-free token-gated slab allocator.
//!
//! Implements a slab allocator where memory blocks are managed in pages.
//! Access is protected by `GhostToken`, allowing concurrent access via `SharedGhostToken`.

use crate::{GhostToken};
use crate::alloc::{ConcurrentGhostAlloc, AllocError};
use crate::concurrency::atomic::GhostAtomicUsize;
use core::alloc::Layout;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc};

// Constants
const PAGE_SIZE: usize = 4096;
const MAX_SMALL_SIZE: usize = 2048;

/// A memory page containing blocks of a specific size.
///
/// This struct is embedded at the START of the allocated 4KB page.
#[repr(C)]
struct Page {
    /// Pointer to the next page in the list.
    next: AtomicUsize,
    block_size: usize,
    /// Index of the first free block, combined with a tag to prevent ABA.
    /// Layout: [Tag: 32 bits | Index: 32 bits]
    free_head: AtomicUsize,
    capacity: usize,
}

const TAG_SHIFT: usize = 32;
const INDEX_MASK: usize = (1 << TAG_SHIFT) - 1;
const NONE: usize = INDEX_MASK; // Use max index as None

impl Page {
    fn new(block_size: usize, next_ptr: usize) -> Option<NonNull<Page>> {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            let page_ptr = ptr as *mut Page;

            // Calculate start offset for blocks
            let header_size = std::mem::size_of::<Page>();
            let mut start_offset = header_size;
            let align_mask = block_size - 1;
            if (start_offset & align_mask) != 0 {
                start_offset = (start_offset + align_mask) & !align_mask;
            }

            let available_bytes = PAGE_SIZE - start_offset;
            let capacity = available_bytes / block_size;

            if capacity == 0 {
                dealloc(ptr, layout);
                return None;
            }

            // Initialize free list in the blocks
            let base_ptr = ptr.add(start_offset);
            for i in 0..capacity - 1 {
                let block_ptr = base_ptr.add(i * block_size);
                // Store next index
                *(block_ptr as *mut u32) = (i + 1) as u32;
            }
            let last_block_ptr = base_ptr.add((capacity - 1) * block_size);
            *(last_block_ptr as *mut u32) = NONE as u32;

            // Initialize header
            ptr::write(page_ptr, Page {
                next: AtomicUsize::new(next_ptr),
                block_size,
                // Initial tag is 0, head is 0
                free_head: AtomicUsize::new(0),
                capacity,
            });

            Some(NonNull::new_unchecked(page_ptr))
        }
    }

    /// Allocates a block from this page using lock-free CAS.
    fn alloc(&self) -> Option<NonNull<u8>> {
        let mut current = self.free_head.load(Ordering::Acquire);

        loop {
            let (idx, tag) = Self::unpack(current);
            if idx == NONE {
                return None;
            }

            // Calculate block address
            unsafe {
                let page_addr = self as *const Page as usize;
                let header_size = std::mem::size_of::<Page>();
                let align_mask = self.block_size - 1;
                let start_offset = (header_size + align_mask) & !align_mask;
                let block_offset = start_offset + idx * self.block_size;
                let block_ptr = (page_addr + block_offset) as *mut u8;

                // Read next index from block
                // We use u32 for index storage in block
                let next_idx = *(block_ptr as *const u32) as usize;

                // New head: next_idx with incremented tag
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

    /// Deallocates a block.
    unsafe fn dealloc(&self, ptr: NonNull<u8>) {
        let page_addr = self as *const Page as usize;
        let ptr_addr = ptr.as_ptr() as usize;

        let header_size = std::mem::size_of::<Page>();
        let align_mask = self.block_size - 1;
        let start_offset = (header_size + align_mask) & !align_mask;

        let offset = ptr_addr - page_addr - start_offset;
        let idx = offset / self.block_size;

        let mut current = self.free_head.load(Ordering::Acquire);
        loop {
            let (_, tag) = Self::unpack(current);
            // We increment tag on push too?
            // Actually, we just need to ensure that if we pop this index later, the tag is different.
            // But we only increment tag on POP usually to distinguish the "same index, different content" state.
            // On PUSH, we just link it.
            // Wait, standard ABA fix is increment tag on every update.
            let new_tag = tag.wrapping_add(1);
            let new_head = Self::pack(idx, new_tag);

            // Link this block to current head index
            let block_ptr = ptr.as_ptr();
            let (curr_head_idx, _) = Self::unpack(current);
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

/// A branded, concurrent slab allocator.
pub struct ConcurrentBrandedSlab<'brand> {
    // Array of atomic pointers to the first page of each size class.
    heads: [GhostAtomicUsize<'brand>; 9],
}

impl<'brand> ConcurrentBrandedSlab<'brand> {
    /// Creates a new concurrent slab allocator.
    pub const fn new() -> Self {
        Self {
            heads: [
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
                GhostAtomicUsize::new(0),
            ],
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

impl<'brand> Default for ConcurrentBrandedSlab<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> ConcurrentGhostAlloc<'brand> for ConcurrentBrandedSlab<'brand> {
    fn allocate(
        &self,
        _token: &GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if let Some(class_idx) = Self::get_class_index(size) {
            let head_atomic = &self.heads[class_idx];

            // 1. Try to allocate from existing pages
            let mut page_ptr_val = head_atomic.load(Ordering::Acquire);
            while page_ptr_val != 0 {
                unsafe {
                    let page = &*(page_ptr_val as *const Page);
                    if let Some(ptr) = page.alloc() {
                        return Ok(ptr);
                    }
                    page_ptr_val = page.next.load(Ordering::Acquire);
                }
            }

            // 2. No space found, allocate new page
            let block_size = Self::get_block_size(class_idx);

            // We loop here to support concurrent growth: multiple threads might try to add a page.
            loop {
                // Load current head again
                let current_head = head_atomic.load(Ordering::Acquire);

                // Allocate new page pointing to current_head
                if let Some(mut new_page) = Page::new(block_size, current_head) {
                    let new_page_val = new_page.as_ptr() as usize;

                    // CAS head to new page
                    match head_atomic.compare_exchange(
                        current_head,
                        new_page_val,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            // Successfully added page. Allocate from it.
                            unsafe {
                                let page = new_page.as_ref();
                                return page.alloc().ok_or(AllocError);
                            }
                        }
                        Err(_) => {
                            // CAS failed, someone else added a page.
                            // We allocated a page for nothing. Clean it up.
                            unsafe {
                                let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
                                dealloc(new_page.as_ptr() as *mut u8, layout);
                            }
                            // Loop again to try adding or using the new head
                        }
                    }
                } else {
                    return Err(AllocError);
                }
            }

        } else {
            // Large allocation
            // Note: For true lock-free concurrent large alloc, we rely on system allocator.
            // We just don't track it in the slab.
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

        if Self::get_class_index(size).is_some() {
            let page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_ref();
            page.dealloc(ptr);
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }
}

// Ensure pages are dropped when slab is dropped?
// Since ConcurrentBrandedSlab uses GhostAtomicUsize, it effectively owns the pages
// but GhostAtomicUsize doesn't have Drop.
// However, ConcurrentBrandedSlab is usually owned by something that lives as long as the brand.
// If the brand scope ends, the Slab is dropped.
// We need to implement Drop to free the pages.
// BUT `GhostAtomicUsize` does not provide access to value in Drop unless we load it.
// And `GhostToken` is gone.
// Actually, `GhostAtomicUsize` contents are just integers.
// We can unsafe access them or just load them with Relaxed if we are in Drop (exclusive access).
// Wait, `GhostAtomicUsize` requires brand to access?
// Yes, `load` requires `Ordering`. It doesn't require token.
// The methods are `load(&self, ...)`.
// So we can walk the list in Drop.

impl<'brand> Drop for ConcurrentBrandedSlab<'brand> {
    fn drop(&mut self) {
        let layout = unsafe { Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE) };

        for head_atomic in &self.heads {
            let mut page_ptr_val = head_atomic.load(Ordering::Acquire);
            while page_ptr_val != 0 {
                unsafe {
                    let page = &*(page_ptr_val as *const Page);
                    let next_val = page.next.load(Ordering::Acquire);

                    // Free the page
                    dealloc(page_ptr_val as *mut u8, layout);

                    page_ptr_val = next_val;
                }
            }
        }
    }
}
