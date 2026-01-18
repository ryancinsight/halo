use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::alloc::{alloc, dealloc, handle_alloc_error};

use crate::GhostCell;
use crate::alloc::allocator::{GhostAlloc, AllocError};

/// A chunk of memory in the bump allocator.
struct Chunk {
    ptr: NonNull<u8>,
    layout: Layout,
    allocated: usize,
}

impl Chunk {
    fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, 16).unwrap(); // 16-byte alignment default for chunks
        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                handle_alloc_error(layout);
            }
            Self {
                ptr: NonNull::new_unchecked(ptr),
                layout,
                allocated: 0,
            }
        }
    }

    fn try_alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let start = self.ptr.as_ptr() as usize;
        let current = start + self.allocated;

        // align current
        let align_offset = (current as *const u8).align_offset(layout.align());
        if align_offset == usize::MAX {
            return None; // Should not happen with typical alignments
        }

        let aligned_current = current + align_offset;
        let end = aligned_current + layout.size();

        if end <= start + self.layout.size() {
            self.allocated = end - start;
            unsafe { Some(NonNull::new_unchecked(aligned_current as *mut u8)) }
        } else {
            None
        }
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

/// A branded bump allocator.
///
/// Allows efficient allocation of heterogeneous types within a branded lifetime.
///
/// # Safety
/// The allocated references live for `'brand`. The allocator must not be dropped
/// while `'brand` is active. Since `'brand` is usually an invariant lifetime
/// of a `GhostToken` or scope, and the allocator is likely created within that scope
/// or passed to it, care must be taken.
///
/// Typically, you would use this with a `GhostToken` where the allocator has the same brand.
pub struct BrandedBumpAllocator<'brand> {
    chunks: UnsafeCell<Vec<Chunk>>,
    current: UnsafeCell<Option<Chunk>>,
    _marker: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> BrandedBumpAllocator<'brand> {
    /// Creates a new branded bump allocator.
    pub fn new() -> Self {
        Self {
            chunks: UnsafeCell::new(Vec::new()),
            current: UnsafeCell::new(None),
            _marker: PhantomData,
        }
    }

    /// Allocates a value and returns a mutable reference.
    pub fn alloc<'a, T>(&'a self, value: T) -> &'a mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc_layout(layout);
        unsafe {
            let ptr = ptr.as_ptr() as *mut T;
            ptr::write(ptr, value);
            &mut *ptr
        }
    }

    /// Allocates a value wrapped in a `GhostCell`.
    pub fn alloc_cell<'a, T>(&'a self, value: T) -> &'a GhostCell<'brand, T> {
        let ptr = self.alloc(GhostCell::new(value));
        &*ptr
    }

    /// Allocates a string slice.
    pub fn alloc_str<'a>(&'a self, s: &str) -> &'a str {
        let layout = Layout::for_value(s);
        let ptr = self.alloc_layout(layout);
        unsafe {
            let ptr = ptr.as_ptr() as *mut u8;
            ptr::copy_nonoverlapping(s.as_ptr(), ptr, s.len());
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, s.len()))
        }
    }

    /// Allocates a copy of a slice.
    pub fn alloc_slice_copy<'a, T: Copy>(&'a self, slice: &[T]) -> &'a [T] {
        let layout = Layout::for_value(slice);
        let ptr = self.alloc_layout(layout);
        unsafe {
            let ptr = ptr.as_ptr() as *mut T;
            ptr::copy_nonoverlapping(slice.as_ptr(), ptr, slice.len());
            core::slice::from_raw_parts(ptr, slice.len())
        }
    }

    fn alloc_layout(&self, layout: Layout) -> NonNull<u8> {
        unsafe {
            let current = &mut *self.current.get();

            if let Some(chunk) = current {
                if let Some(ptr) = chunk.try_alloc(layout) {
                    return ptr;
                }
                // Chunk full, move to chunks list
                let full_chunk = std::mem::replace(chunk, Chunk::new(Self::next_chunk_size(chunk.layout.size())));
                (*self.chunks.get()).push(full_chunk);
            } else {
                *current = Some(Chunk::new(1024));
            }

            // Try again with new chunk
            current.as_mut().unwrap().try_alloc(layout).expect("Allocation failed even in new chunk")
        }
    }

    fn next_chunk_size(current_size: usize) -> usize {
        (current_size * 2).min(1024 * 1024) // Cap at 1MB chunks
    }

    /// Resets the allocator, clearing all allocations.
    ///
    /// # Safety
    /// This is unsafe because it invalidates all references `'brand`.
    /// However, `'brand` is a lifetime. You cannot "clear" it safely while references exist.
    /// Thus this method requires `&mut self`, implying exclusive access.
    /// If exclusive access exists, no other references should exist (conceptually),
    /// but with `'brand` being invariant and potentially longer than the borrow,
    /// this is tricky.
    ///
    /// Ideally, `reset` should only be called if we can change the brand or if we know no one uses the old data.
    /// But since `BrandedBumpAllocator` is tied to `'brand` structurally, `reset` is only safe if we know
    /// all `'brand` references are dead.
    pub unsafe fn reset(&mut self) {
        let chunks = self.chunks.get_mut();
        chunks.clear();
        *self.current.get_mut() = None;
    }
}

impl<'brand> Default for BrandedBumpAllocator<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<'brand> Send for BrandedBumpAllocator<'brand> {}
// Not Sync because it uses UnsafeCell without synchronization.

impl<'brand> GhostAlloc<'brand> for BrandedBumpAllocator<'brand> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        Ok(self.alloc_layout(layout))
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        // No-op: memory is freed when allocator is dropped
    }
}
