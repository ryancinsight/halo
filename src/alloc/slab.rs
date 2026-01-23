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

/// A memory page containing blocks of a specific size.
struct Page {
    memory: NonNull<u8>,
    capacity: usize, // Number of blocks
    block_size: usize,
    free_head: Option<usize>, // Index of the first free block
    free_count: usize,
    next: Option<Box<Page>>, // Linked list of pages
}

impl Page {
    fn new(block_size: usize) -> Option<Box<Self>> {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;
        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }
            let non_null = NonNull::new_unchecked(ptr);

            let capacity = PAGE_SIZE / block_size;
            let mut page = Box::new(Page {
                memory: non_null,
                capacity,
                block_size,
                free_head: Some(0),
                free_count: capacity,
                next: None,
            });

            // Initialize free list in the page
            // We store the next free index in the first usize of each block.
            // Requirement: block_size >= size_of::<usize>()
            let mut p = ptr;
            for i in 0..capacity - 1 {
                *(p as *mut usize) = i + 1;
                p = p.add(block_size);
            }
            *(p as *mut usize) = usize::MAX; // Sentinel

            Some(page)
        }
    }

    unsafe fn alloc(&mut self) -> Option<NonNull<u8>> {
        if let Some(idx) = self.free_head {
            let offset = idx * self.block_size;
            let ptr = self.memory.as_ptr().add(offset);

            // Read next free index
            let next = *(ptr as *const usize);
            self.free_head = if next == usize::MAX { None } else { Some(next) };
            self.free_count -= 1;

            Some(NonNull::new_unchecked(ptr))
        } else {
            None
        }
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>) {
        let offset = ptr.as_ptr() as usize - self.memory.as_ptr() as usize;
        let idx = offset / self.block_size;

        // Add to free list
        *(ptr.as_ptr() as *mut usize) = self.free_head.unwrap_or(usize::MAX);
        self.free_head = Some(idx);
        self.free_count += 1;
    }

    fn contains(&self, ptr: NonNull<u8>) -> bool {
        let start = self.memory.as_ptr() as usize;
        let end = start + PAGE_SIZE;
        let p = ptr.as_ptr() as usize;
        p >= start && p < end
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE);
            dealloc(self.memory.as_ptr(), layout);
        }
    }
}

/// Internal state of the slab allocator.
struct SlabState {
    // Array of page lists, one for each size class (powers of 2, starting at 8)
    // Indices: 0->8, 1->16, 2->32, ... 8->2048
    pages: [Option<Box<Page>>; 9],
}

impl SlabState {
    fn new() -> Self {
        Self {
            pages: Default::default(),
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

            // Try to find a page with space
            let mut curr = &mut state.pages[class_idx];

            // First check if head has space
            if let Some(page) = curr {
                if page.free_count > 0 {
                    unsafe {
                        if let Some(ptr) = page.alloc() {
                            return Ok(ptr);
                        }
                    }
                }
            }

            // Iterate to find a page (simplified: we just allocate a new page if head is full for now,
            // but a real implementation would search or move full pages to back)
            // For this implementation, let's just prepend a new page if the head is full.

            if let Some(mut new_page) = Page::new(block_size) {
                unsafe {
                    let ptr = new_page.alloc().ok_or(AllocError)?;
                    new_page.next = state.pages[class_idx].take();
                    state.pages[class_idx] = Some(new_page);
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
        let state = self.state.borrow_mut(token);

        if let Some(class_idx) = SlabState::get_class_index(size) {
            let mut curr = &mut state.pages[class_idx];
            while let Some(page) = curr {
                if page.contains(ptr) {
                    page.dealloc(ptr);
                    return;
                }
                curr = &mut page.next;
            }
            // If not found in pages, it might have been a large alloc or error?
            // But wait, if we support large allocs via global, we need to know if it was large.
            // Our get_class_index handles that logic.
            // Ideally we would know if it belongs to us.
            // Since we don't track large allocs in the state, if we fall through here, it implies logic error or mixed allocators?
            // Actually, if size > MAX_SMALL_SIZE, get_class_index returns None, so we go to else.
            // If size is small, we assume it's in our pages.
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
}
