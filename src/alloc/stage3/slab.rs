use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use core::alloc::Layout;
use std::alloc::{alloc, dealloc};
use crate::GhostToken;
use crate::token::traits::GhostBorrow;
use crate::alloc::stage3::freelist::BrandedFreelist;

/// A slab allocator managing a single fixed-size page.
///
/// The `BrandedSlab` struct is embedded at the beginning of the 4KB page.
/// Objects are allocated from the remaining space.
#[repr(C)]
pub struct BrandedSlab<'brand, const OBJECT_SIZE: usize, const OBJECTS_PER_SLAB: usize> {
    // Linked list next pointer (for SizeClassManager lists).
    // Must be first to allow BrandedFreelist to use it as the link.
    // We use AtomicUsize to ensure size/alignment matches pointer.
    pub next_slab: AtomicUsize,

    // Linked list for tracking ALL slabs (for Drop).
    // We need a second link field.
    // But BrandedFreelist expects to write to offset 0.
    // We can't use BrandedFreelist for both lists unless we use an offset-aware list.
    // Or we use a manually managed stack for 'all_slabs'.
    pub next_all: AtomicUsize,

    // Internal freelist for objects
    freelist: BrandedFreelist<'brand>,
    bump_index: AtomicUsize,
    alloc_cnt: AtomicUsize,
    // We need to know the page start to calculate offsets?
    // effectively `self` is the page start.
    _marker: core::marker::PhantomData<&'brand ()>,
    // Padding to ensure alignment of objects?
    // The struct size depends on fields.
    // We should probably pad to something?
    // For now, we assume standard layout.
}

// Safety: All internal state is atomic or lock-free.
unsafe impl<'brand, const S: usize, const N: usize> Send for BrandedSlab<'brand, S, N> {}
unsafe impl<'brand, const S: usize, const N: usize> Sync for BrandedSlab<'brand, S, N> {}

impl<'brand, const OBJECT_SIZE: usize, const OBJECTS_PER_SLAB: usize> BrandedSlab<'brand, OBJECT_SIZE, OBJECTS_PER_SLAB> {
    // Constants
    const PAGE_SIZE: usize = 4096;

    /// Creates a new slab.
    pub fn new() -> Option<NonNull<Self>> {
        if OBJECT_SIZE < core::mem::size_of::<usize>() {
            return None;
        }

        let header_size = core::mem::size_of::<Self>();
        let available = Self::PAGE_SIZE - header_size;
        let capacity = available / OBJECT_SIZE;

        if capacity < OBJECTS_PER_SLAB {
            // The requested N is too large for the page overhead
            return None;
        }

        unsafe {
            let layout = Layout::from_size_align_unchecked(Self::PAGE_SIZE, Self::PAGE_SIZE);
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            let slab_ptr = ptr as *mut Self;

            // Initialize in place
            // next_slab
            core::ptr::write(&mut (*slab_ptr).next_slab, AtomicUsize::new(0));
            // next_all
            core::ptr::write(&mut (*slab_ptr).next_all, AtomicUsize::new(0));
            // freelist
            core::ptr::write(&mut (*slab_ptr).freelist, BrandedFreelist::new());
            // bump_index
            core::ptr::write(&mut (*slab_ptr).bump_index, AtomicUsize::new(0));
            // alloc_cnt
            core::ptr::write(&mut (*slab_ptr).alloc_cnt, AtomicUsize::new(0));
            // marker
            core::ptr::write(&mut (*slab_ptr)._marker, core::marker::PhantomData);

            Some(NonNull::new_unchecked(slab_ptr))
        }
    }

    /// Helper to get the start of the object area.
    fn object_area_start(&self) -> usize {
        let self_addr = self as *const _ as usize;
        let header_size = core::mem::size_of::<Self>();
        // Align to OBJECT_SIZE? Or just header size?
        // Usually we want objects aligned to OBJECT_SIZE or at least word aligned.
        // Let's align to 16 bytes or OBJECT_SIZE.
        // For simplicity, just after header.
        let mut start = self_addr + header_size;

        // Align start to OBJECT_SIZE if power of 2?
        // Let's align to 16 bytes for now.
        let align = 16;
        if start % align != 0 {
            start = (start + align) & !(align - 1);
        }
        start
    }

    /// Allocates an object from the slab.
    pub fn alloc(&self, token: &impl GhostBorrow<'brand>) -> Option<*mut u8> {
        // 1. Try freelist
        if let Some(ptr) = unsafe { self.freelist.pop(token) } {
            self.alloc_cnt.fetch_add(1, Ordering::Relaxed);
            return Some(ptr);
        }

        // 2. Try bump
        loop {
            let idx = self.bump_index.load(Ordering::Relaxed);
            if idx >= OBJECTS_PER_SLAB {
                return None;
            }

            if self.bump_index.compare_exchange(
                idx,
                idx + 1,
                Ordering::AcqRel,
                Ordering::Relaxed
            ).is_ok() {
                let start_addr = self.object_area_start();
                let offset = idx * OBJECT_SIZE;

                // Safety check
                if offset + OBJECT_SIZE > (Self::PAGE_SIZE - (start_addr - (self as *const _ as usize))) {
                     return None;
                }

                let ptr = (start_addr + offset) as *mut u8;
                self.alloc_cnt.fetch_add(1, Ordering::Relaxed);
                return Some(ptr);
            }
        }
    }

    /// Allocates contiguous bump objects.
    pub fn alloc_bump_batch(&self, _token: &impl GhostBorrow<'brand>, count: usize) -> Option<*mut u8> {
        loop {
            let idx = self.bump_index.load(Ordering::Relaxed);
            if idx + count > OBJECTS_PER_SLAB {
                return None;
            }
            if self.bump_index.compare_exchange(
                idx,
                idx + count,
                Ordering::AcqRel,
                Ordering::Relaxed
            ).is_ok() {
                let start_addr = self.object_area_start();
                let offset = idx * OBJECT_SIZE;

                if offset + count * OBJECT_SIZE > (Self::PAGE_SIZE - (start_addr - (self as *const _ as usize))) {
                     return None;
                }

                let ptr = (start_addr + offset) as *mut u8;
                self.alloc_cnt.fetch_add(count, Ordering::Relaxed);
                return Some(ptr);
            }
        }
    }

    /// Frees an object.
    pub unsafe fn free(&self, token: &impl GhostBorrow<'brand>, ptr: *mut u8) {
        self.freelist.push(token, ptr);
        self.alloc_cnt.fetch_sub(1, Ordering::Relaxed);
    }

    /// Frees a batch of objects.
    pub unsafe fn free_batch<I>(&self, token: &impl GhostBorrow<'brand>, iter: I)
    where I: IntoIterator<Item = *mut u8>
    {
        let count = self.freelist.push_batch(token, iter);
        if count > 0 {
            self.alloc_cnt.fetch_sub(count, Ordering::Relaxed);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.alloc_cnt.load(Ordering::Relaxed) == 0
    }

    pub fn is_full(&self) -> bool {
        self.alloc_cnt.load(Ordering::Relaxed) >= OBJECTS_PER_SLAB
    }

    pub fn allocated_count(&self) -> usize {
        self.alloc_cnt.load(Ordering::Relaxed)
    }

    /// Recover the Slab reference from a pointer allocated within it.
    /// Assumes slab is 4KB aligned.
    pub unsafe fn from_ptr(ptr: *mut u8) -> NonNull<Self> {
        let addr = ptr as usize;
        let page_addr = addr & !(Self::PAGE_SIZE - 1);
        NonNull::new_unchecked(page_addr as *mut Self)
    }
}
