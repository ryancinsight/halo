use super::super::syscall;
use super::super::constants::PAGE_SIZE;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A simple bump allocator managing a large virtual memory region.
/// Used during the bootstrap phase to seed the global allocator.
pub struct BootstrapArena {
    start: *mut u8,
    size: usize,
    cursor: AtomicUsize,
}

impl BootstrapArena {
    /// Acquires a new virtual memory region of `size` bytes.
    pub fn new(size: usize) -> Option<Self> {
        unsafe {
            let ptr = syscall::allocate_region(size)?;
            Some(Self {
                start: ptr,
                size,
                cursor: AtomicUsize::new(0),
            })
        }
    }

    /// Allocates a single page from the arena.
    /// Thread-safe via atomic cursor.
    pub fn alloc_page(&self) -> Option<*mut u8> {
        let offset = self.cursor.fetch_add(PAGE_SIZE, Ordering::SeqCst);
        if offset + PAGE_SIZE > self.size {
            return None;
        }
        unsafe { Some(self.start.add(offset)) }
    }

    /// Returns the capacity of the arena.
    pub fn capacity(&self) -> usize {
        self.size
    }

    /// Returns the currently used size.
    pub fn used(&self) -> usize {
        self.cursor.load(Ordering::SeqCst)
    }
}

unsafe impl Send for BootstrapArena {}
unsafe impl Sync for BootstrapArena {}

impl Drop for BootstrapArena {
    fn drop(&mut self) {
        unsafe {
            syscall::free_region(self.start, self.size);
        }
    }
}
