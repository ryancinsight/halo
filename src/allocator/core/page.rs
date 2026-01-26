use crate::alloc::page::PageAlloc;
use crate::allocator::syscall::{allocate_region, free_region};
use core::alloc::Layout;
use std::sync::Mutex;

/// A page allocator that uses direct syscalls (mmap/VirtualAlloc)
/// and caches pages globally to reduce syscall overhead.
#[derive(Default, Clone, Copy, Debug)]
pub struct SyscallPageAlloc;

struct PageHeap {
    head: *mut u8,
}

unsafe impl Send for PageHeap {}

static PAGE_HEAP: Mutex<PageHeap> = Mutex::new(PageHeap { head: core::ptr::null_mut() });

impl PageAlloc for SyscallPageAlloc {
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8 {
        debug_assert_eq!(layout.size(), 4096);
        debug_assert_eq!(layout.align(), 4096);

        // 1. Try pop from global heap
        {
            let mut heap = PAGE_HEAP.lock().unwrap();
            if !heap.head.is_null() {
                let ptr = heap.head;
                // Read next pointer from the page (embedded at offset 0)
                let next = *(ptr as *mut *mut u8);
                heap.head = next;
                return ptr;
            }
        }

        // 2. Allocate new chunk (256KB = 64 pages)
        const CHUNK_PAGES: usize = 64;
        const PAGE_SIZE: usize = 4096;
        let chunk_size = CHUNK_PAGES * PAGE_SIZE;

        if let Some(chunk) = allocate_region(chunk_size) {
            // Return first page, add rest to heap
            let mut heap = PAGE_HEAP.lock().unwrap();
            for i in 1..CHUNK_PAGES {
                let p = chunk.add(i * PAGE_SIZE);
                *(p as *mut *mut u8) = heap.head;
                heap.head = p;
            }
            return chunk;
        }

        core::ptr::null_mut()
    }

    unsafe fn dealloc_page(&self, ptr: *mut u8, _layout: Layout) {
        // Return to global heap
        let mut heap = PAGE_HEAP.lock().unwrap();
        *(ptr as *mut *mut u8) = heap.head;
        heap.head = ptr;
        // Note: We never unmap here. Memory grows.
        // For a system allocator, this is acceptable behavior (retaining memory).
        // Or we could track chunks and free if empty, but that requires more metadata.
    }
}
