//! `BrandedSlab` — a token-gated slab allocator.
//!
//! Implements a slab allocator where memory blocks are managed in pages.
//! Access is protected by `GhostToken`, ensuring exclusive access without locks.

use crate::{GhostToken, GhostCell};
use crate::alloc::{GhostAlloc, AllocError};
use core::alloc::Layout;
use core::ptr::NonNull;
use std::alloc::{alloc, dealloc, handle_alloc_error};

// Constants
const PAGE_SIZE: usize = 4096;
const MAX_SMALL_SIZE: usize = 2048; // Anything larger goes to global allocator

// TODO: Support custom page sizes or huge pages for better performance with large working sets.

/// A memory page containing blocks of a specific size.
///
/// This struct is embedded at the START of the allocated 4KB page.
#[repr(C)]
struct Page {
    next: Option<NonNull<Page>>, // Linked list of pages
    block_size: usize,
    free_head: Option<usize>, // Index of the first free block (relative to first block)
    free_count: usize,
    capacity: usize,
    // The actual blocks follow this struct in memory.
}

impl Page {
    /// Allocates a new 4KB page and initializes it as a Page with blocks of `block_size`.
    fn new(block_size: usize) -> Option<NonNull<Page>> {
        // Ensure alignment is 4KB so we can find the header via masking
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            let page_ptr = ptr as *mut Page;

            // Calculate where blocks start
            // We need to ensure the first block is aligned to block_size
            let header_size = std::mem::size_of::<Page>();
            let mut start_offset = header_size;

            // Align start_offset to block_size (assuming block_size is power of 2)
            let align_mask = block_size - 1;
            if (start_offset & align_mask) != 0 {
                start_offset = (start_offset + align_mask) & !align_mask;
            }

            if start_offset >= PAGE_SIZE {
                // Header too big for this block size (shouldn't happen for reasonable sizes)
                dealloc(ptr, layout);
                return None;
            }

            let available_bytes = PAGE_SIZE - start_offset;
            let capacity = available_bytes / block_size;

            if capacity == 0 {
                dealloc(ptr, layout);
                return None;
            }

            // Write the header
            ptr::write(page_ptr, Page {
                next: None,
                block_size,
                free_head: Some(0),
                free_count: capacity,
                capacity,
            });

            // Initialize free list
            // Blocks are indexed 0..capacity.
            // We write the index of the next free block into the block memory itself.
            let base_ptr = ptr.add(start_offset);
            for i in 0..capacity - 1 {
                let block_ptr = base_ptr.add(i * block_size);
                *(block_ptr as *mut usize) = i + 1;
            }
            // Last block
            let last_block_ptr = base_ptr.add((capacity - 1) * block_size);
            *(last_block_ptr as *mut usize) = usize::MAX;

            Some(NonNull::new_unchecked(page_ptr))
        }
    }

    /// Allocates a block from this page.
    unsafe fn alloc(&mut self) -> Option<NonNull<u8>> {
        // TODO: Implement a more sophisticated free list search (e.g., bitmask or intrusive list) for better locality.
        if let Some(idx) = self.free_head {
            let page_addr = self as *mut Page as usize;

            // Re-calculate start offset
            let header_size = std::mem::size_of::<Page>();
            let align_mask = self.block_size - 1;
            let start_offset = (header_size + align_mask) & !align_mask;

            let block_offset = start_offset + idx * self.block_size;
            let ptr = (page_addr + block_offset) as *mut u8;

            // Read next free index from the block
            let next = *(ptr as *const usize);
            self.free_head = if next == usize::MAX { None } else { Some(next) };
            self.free_count -= 1;

            Some(NonNull::new_unchecked(ptr))
        } else {
            None
        }
    }

    /// Deallocates a block in this page.
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>) {
        let page_addr = self as *mut Page as usize;
        let ptr_addr = ptr.as_ptr() as usize;

        // Re-calculate start offset
        let header_size = std::mem::size_of::<Page>();
        let align_mask = self.block_size - 1;
        let start_offset = (header_size + align_mask) & !align_mask;

        let offset = ptr_addr - page_addr - start_offset;
        let idx = offset / self.block_size;

        // Add to free list
        *(ptr.as_ptr() as *mut usize) = self.free_head.unwrap_or(usize::MAX);
        self.free_head = Some(idx);
        self.free_count += 1;
    }

    // Helper to get page from any pointer inside it
    unsafe fn from_ptr(ptr: NonNull<u8>) -> NonNull<Page> {
        let addr = ptr.as_ptr() as usize;
        let page_addr = addr & !(PAGE_SIZE - 1);
        NonNull::new_unchecked(page_addr as *mut Page)
    }
}

use core::ptr;

/// Internal state of the slab allocator.
struct SlabState {
    // Array of page lists, one for each size class (powers of 2, starting at 8)
    pages: [Option<NonNull<Page>>; 9],
}

impl SlabState {
    fn new() -> Self {
        Self {
            pages: [None; 9],
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
            for i in 0..9 {
                let mut curr = self.pages[i];
                while let Some(mut page_ptr) = curr {
                    let page = page_ptr.as_mut();
                    let next = page.next;
                    // Drop the page memory
                    // We don't drop Page contents because they are POD + raw pointers, nothing to drop.
                    dealloc(page_ptr.as_ptr() as *mut u8, layout);
                    curr = next;
                }
            }
        }
    }
}

/// A branded slab allocator.
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
}

// TODO: Add thread-local caching or stealing mechanisms if we extend SharedGhostToken usage.

impl<'brand> Default for BrandedSlab<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> GhostAlloc<'brand> for BrandedSlab<'brand> {
    fn allocate(
        &self,
        token: &mut GhostToken<'brand>,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        let state = self.state.borrow_mut(token);

        if let Some(class_idx) = SlabState::get_class_index(size) {
            let block_size = SlabState::get_block_size(class_idx);

            // Try head page
            if let Some(mut page_ptr) = state.pages[class_idx] {
                unsafe {
                    let page = page_ptr.as_mut();
                    if page.free_count > 0 {
                        if let Some(ptr) = page.alloc() {
                            return Ok(ptr);
                        }
                    }
                }
            }

            // Head full or empty, allocate new page
            // Optimization: We could search the list, but for now we just push a new page to front
            // if head is full.
            if let Some(mut new_page_ptr) = Page::new(block_size) {
                unsafe {
                    let new_page = new_page_ptr.as_mut();
                    let ptr = new_page.alloc().ok_or(AllocError)?;

                    new_page.next = state.pages[class_idx];
                    state.pages[class_idx] = Some(new_page_ptr);

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

    unsafe fn deallocate(
        &self,
        token: &mut GhostToken<'brand>,
        ptr: NonNull<u8>,
        layout: Layout,
    ) {
        let size = layout.size().max(layout.align()).max(std::mem::size_of::<usize>());

        if SlabState::get_class_index(size).is_some() {
            // It's a small allocation, so it belongs to a Page.
            // O(1) retrieval of Page
            let mut page_ptr = Page::from_ptr(ptr);
            let page = page_ptr.as_mut();

            // Safety check (debug only?): ensure block size matches
            // debug_assert_eq!(page.block_size, SlabState::get_block_size(SlabState::get_class_index(size).unwrap()));

            page.dealloc(ptr);

            // Note: We don't eagerly return empty pages to OS in this simple implementation,
            // nor do we maintain a "partial" list vs "full" list.
            // This is a basic slab.
            // TODO: Implement eager return of empty pages to the OS to reduce memory pressure.
        } else {
            dealloc(ptr.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_slab_basic() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>(); // 8 bytes

            // Allocate
            let ptr1 = slab.allocate(&mut token, layout).unwrap();
            let ptr2 = slab.allocate(&mut token, layout).unwrap();

            unsafe {
                *(ptr1.as_ptr() as *mut u64) = 123;
                *(ptr2.as_ptr() as *mut u64) = 456;
                assert_eq!(*(ptr1.as_ptr() as *mut u64), 123);
                assert_eq!(*(ptr2.as_ptr() as *mut u64), 456);

                // Deallocate
                slab.deallocate(&mut token, ptr1, layout);
                slab.deallocate(&mut token, ptr2, layout);
            }
        });
    }

    #[test]
    fn test_branded_slab_reuse() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u64>();

            let ptr1 = slab.allocate(&mut token, layout).unwrap();
            let addr1 = ptr1.as_ptr() as usize;

            unsafe { slab.deallocate(&mut token, ptr1, layout); }

            let ptr2 = slab.allocate(&mut token, layout).unwrap();
            let addr2 = ptr2.as_ptr() as usize;

            // Simple LIFO behavior expected from free head
            assert_eq!(addr1, addr2);
        });
    }

    #[test]
    fn test_large_alloc() {
         GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::from_size_align(10000, 16).unwrap();

            let ptr = slab.allocate(&mut token, layout).unwrap();
            unsafe {
                slab.deallocate(&mut token, ptr, layout);
            }
         });
    }

    #[test]
    fn test_page_alignment_and_access() {
        GhostToken::new(|mut token| {
            let slab = BrandedSlab::new();
            let layout = Layout::new::<u32>(); // 4 bytes -> class 8 bytes

            let ptr = slab.allocate(&mut token, layout).unwrap();

            // Verify alignment
            let addr = ptr.as_ptr() as usize;
            let page_addr = addr & !(PAGE_SIZE - 1);

            // Read header
            unsafe {
                let page_ptr = page_addr as *const Page;
                let page = &*page_ptr;
                assert_eq!(page.block_size, 8);
                // Capacity should be ~ (4096 - sizeof(Page)) / 8
                // sizeof(Page) is roughly 40-48 bytes?
                // 4096 - 48 = 4048 / 8 = 506.
                assert!(page.capacity > 400);
            }

            unsafe { slab.deallocate(&mut token, ptr, layout); }
        });
    }
}
