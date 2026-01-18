use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::alloc::{alloc, dealloc, handle_alloc_error};

use crate::{GhostCell, GhostToken, GhostUnsafeCell};
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
    chunks: GhostUnsafeCell<'brand, Vec<Chunk>>,
    current: GhostUnsafeCell<'brand, Option<Chunk>>,
    _marker: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> BrandedBumpAllocator<'brand> {
    /// Creates a new branded bump allocator.
    pub fn new() -> Self {
        Self {
            chunks: GhostUnsafeCell::new(Vec::new()),
            current: GhostUnsafeCell::new(None),
            _marker: PhantomData,
        }
    }

    /// Allocates a value and returns a mutable reference.
    pub fn alloc<'a, T>(&'a self, value: T, token: &mut GhostToken<'brand>) -> &'a mut T {
        let layout = Layout::new::<T>();
        let ptr = self.alloc_layout(layout, token);
        unsafe {
            let ptr = ptr.as_ptr() as *mut T;
            ptr::write(ptr, value);
            &mut *ptr
        }
    }

    /// Allocates a value wrapped in a `GhostCell`.
    pub fn alloc_cell<'a, T>(&'a self, value: T, token: &mut GhostToken<'brand>) -> &'a GhostCell<'brand, T> {
        let ptr = self.alloc(GhostCell::new(value), token);
        &*ptr
    }

    /// Allocates a string slice.
    pub fn alloc_str<'a>(&'a self, s: &str, token: &mut GhostToken<'brand>) -> &'a str {
        let layout = Layout::for_value(s);
        let ptr = self.alloc_layout(layout, token);
        unsafe {
            let ptr = ptr.as_ptr() as *mut u8;
            ptr::copy_nonoverlapping(s.as_ptr(), ptr, s.len());
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, s.len()))
        }
    }

    /// Allocates a copy of a slice.
    pub fn alloc_slice_copy<'a, T: Copy>(&'a self, slice: &[T], token: &mut GhostToken<'brand>) -> &'a [T] {
        let layout = Layout::for_value(slice);
        let ptr = self.alloc_layout(layout, token);
        unsafe {
            let ptr = ptr.as_ptr() as *mut T;
            ptr::copy_nonoverlapping(slice.as_ptr(), ptr, slice.len());
            core::slice::from_raw_parts(ptr, slice.len())
        }
    }

    fn alloc_layout(&self, layout: Layout, token: &mut GhostToken<'brand>) -> NonNull<u8> {
        // Try to allocate from current chunk
        let full_chunk = {
            let current_slot = self.current.get_mut(token);
            if let Some(chunk) = current_slot {
                if let Some(ptr) = chunk.try_alloc(layout) {
                    return ptr;
                }
                // Chunk is full, take it out to make room for new one
                current_slot.take().unwrap()
            } else {
                // No current chunk, create initial one
                let mut new_chunk = Chunk::new(1024);
                let ptr = new_chunk.try_alloc(layout).expect("Initial allocation failed");
                *current_slot = Some(new_chunk);
                return ptr;
            }
        };

        // If we reach here, `full_chunk` needs to be retired.
        let next_size = Self::next_chunk_size(full_chunk.layout.size());

        // Push full chunk to history
        self.chunks.get_mut(token).push(full_chunk);

        // Create and set new chunk
        let mut new_chunk = Chunk::new(next_size);
        let ptr = new_chunk.try_alloc(layout).expect("Allocation failed even in new chunk");
        *self.current.get_mut(token) = Some(new_chunk);

        ptr
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
    pub unsafe fn reset(&mut self) {
        let chunks = self.chunks.get_mut_exclusive();
        chunks.clear();
        *self.current.get_mut_exclusive() = None;
    }
}

impl<'brand> Default for BrandedBumpAllocator<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<'brand> Send for BrandedBumpAllocator<'brand> {}
// Not Sync because it uses GhostUnsafeCell without synchronization.

impl<'brand> GhostAlloc<'brand> for BrandedBumpAllocator<'brand> {
    fn allocate(&self, token: &mut GhostToken<'brand>, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        Ok(self.alloc_layout(layout, token))
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        // No-op: memory is freed when allocator is dropped
    }
}
