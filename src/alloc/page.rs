use core::alloc::Layout;
use std::alloc::{alloc, dealloc};
use std::sync::Mutex;
use crate::alloc::system::syscall::allocate_region;

pub const PAGE_SIZE: usize = 4096;
pub const fn align_up(value: usize, align: usize) -> usize {
    if align == 0 {
        value
    } else {
        (value + (align - 1)) & !(align - 1)
    }
}

/// A trait for allocating and deallocating pages (4KB aligned/sized).
///
/// This allows `BrandedSlab` to be used with different backing stores,
/// such as the global system allocator (for library usage) or direct syscalls
/// (for the Halo system allocator).
pub trait PageAlloc {
    /// Allocates a page of memory.
    ///
    /// The returned pointer must be aligned to the page size (4096 bytes).
    /// The size of the allocation is determined by `layout.size()`.
    ///
    /// # Safety
    /// Caller must ensure layout is valid.
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8;

    /// Deallocates a page of memory.
    ///
    /// # Safety
    /// Caller must ensure ptr was allocated by this allocator with the given layout.
    unsafe fn dealloc_page(&self, ptr: *mut u8, layout: Layout);
}

/// A page allocator that uses the global system allocator.
#[derive(Default, Clone, Copy, Debug)]
pub struct GlobalPageAlloc;

impl PageAlloc for GlobalPageAlloc {
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8 {
        alloc(layout)
    }

    unsafe fn dealloc_page(&self, ptr: *mut u8, layout: Layout) {
        dealloc(ptr, layout)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct SyscallPageAlloc;

struct PageHeap {
    head: *mut u8,
}

unsafe impl Send for PageHeap {}

static PAGE_HEAP: Mutex<PageHeap> = Mutex::new(PageHeap { head: core::ptr::null_mut() });

impl PageAlloc for SyscallPageAlloc {
    unsafe fn alloc_page(&self, layout: Layout) -> *mut u8 {
        debug_assert_eq!(layout.size(), PAGE_SIZE);
        debug_assert_eq!(layout.align(), PAGE_SIZE);

        {
            let mut heap = PAGE_HEAP.lock().unwrap();
            if !heap.head.is_null() {
                let ptr = heap.head;
                let next = *(ptr as *mut *mut u8);
                heap.head = next;
                return ptr;
            }
        }

        const CHUNK_PAGES: usize = 64;
        let chunk_size = CHUNK_PAGES * PAGE_SIZE;

        if let Some(chunk) = allocate_region(chunk_size) {
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
        let mut heap = PAGE_HEAP.lock().unwrap();
        *(ptr as *mut *mut u8) = heap.head;
        heap.head = ptr;
    }
}
