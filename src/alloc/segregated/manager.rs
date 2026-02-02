use crate::GhostToken;
use crate::token::traits::GhostBorrow;
use crate::alloc::segregated::size_class::SizeClass;
use crate::alloc::segregated::slab::SegregatedSlab;
use crate::alloc::segregated::freelist::BrandedFreelist;
use crate::alloc::page::{PageAlloc, PAGE_SIZE};
use crate::concurrency::sync::{wait_on_u32, wake_one_u32, wake_all_u32};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use core::ptr::{self};
use core::alloc::Layout;
use core::marker::PhantomData;

/// Manages slabs for a specific size class.
/// `SIZE` must be equal to `SC::SIZE`.
pub struct SizeClassManager<'brand, SC: SizeClass, PA: PageAlloc + Default, const SIZE: usize, const N: usize> {
    // Active slab (allocating)
    active: AtomicPtr<SegregatedSlab<'brand, SIZE, N>>,
    // Available slabs (Partial + Empty)
    available: BrandedFreelist<'brand>,
    // Track all allocated slabs to free them on drop
    all_slabs: AtomicPtr<SegregatedSlab<'brand, SIZE, N>>,
    // Lock to serialize slab creation and prevent storming the page allocator
    // 0 = unlocked, 1 = locked
    creation_lock: AtomicU32,
    _marker: PhantomData<(SC, PA)>,
}

unsafe impl<'brand, SC: SizeClass, PA: PageAlloc + Default, const SIZE: usize, const N: usize> Send for SizeClassManager<'brand, SC, PA, SIZE, N> {}
unsafe impl<'brand, SC: SizeClass, PA: PageAlloc + Default, const SIZE: usize, const N: usize> Sync for SizeClassManager<'brand, SC, PA, SIZE, N> {}

impl<'brand, SC: SizeClass, PA: PageAlloc + Default, const SIZE: usize, const N: usize> SizeClassManager<'brand, SC, PA, SIZE, N> {
    pub const fn new() -> Self {
        Self {
            active: AtomicPtr::new(ptr::null_mut()),
            available: BrandedFreelist::new(),
            all_slabs: AtomicPtr::new(ptr::null_mut()),
            creation_lock: AtomicU32::new(0),
            _marker: PhantomData,
        }
    }

    pub fn alloc(&self, token: &impl GhostBorrow<'brand>) -> Option<*mut u8> {
        loop {
            let active_ptr = self.active.load(Ordering::Acquire);

            if !active_ptr.is_null() {
                let slab = unsafe { &*active_ptr };
                if let Some(p) = slab.alloc(token) {
                    return Some(p);
                }
            }

            // Need new active.
            // First, try to pop from available slabs.
            if let Some(slab_ptr) = unsafe { self.available.pop(token) } {
                let new_active = slab_ptr.as_ptr() as *mut SegregatedSlab<'brand, SIZE, N>;
                // Try install new active
                match self.active.compare_exchange(
                    active_ptr,
                    new_active,
                    Ordering::AcqRel,
                    Ordering::Acquire
                ) {
                    Ok(old_ptr) => {
                        // We replaced old_ptr with new_active.
                        // Check if old_ptr needs to be saved (maybe it became non-full?).
                        if old_ptr != ptr::null_mut() {
                             let old_slab = unsafe { &*old_ptr };
                             if !old_slab.is_full() {
                                 unsafe { self.available.push(token, core::ptr::NonNull::new_unchecked(old_ptr as *mut u8)); }
                             }
                        }
                        // Now alloc from the new active
                        let slab = unsafe { &*new_active };
                        if let Some(p) = slab.alloc(token) {
                            return Some(p);
                        }
                        // If failed (should not happen if we just popped it), loop again.
                        continue;
                    },
                    Err(_) => {
                        // Race. Someone else installed active.
                        // Push our popped slab back to available.
                        unsafe { self.available.push(token, core::ptr::NonNull::new_unchecked(new_active as *mut u8)); }
                        continue;
                    }
                }
            }

            // Available is empty. We need to allocate a new slab.
            // Use futex lock to prevent multiple threads from allocating simultaneously.
            if self.creation_lock.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                // Acquired lock.
                // Double check available in case someone pushed while we were acquiring
                if let Some(slab_ptr) = unsafe { self.available.pop(token) } {
                     // Someone filled available. Use it.
                     self.creation_lock.store(0, Ordering::Release);
                     wake_all_u32(&self.creation_lock);
                     
                     // We just use the normal flow by pushing it back and continuing
                     unsafe { self.available.push(token, slab_ptr); }
                     continue; 
                }

                // Alloc new
                let slab = match SegregatedSlab::new_in(&PA::default()) {
                    Some(non_null) => non_null.as_ptr(),
                    None => {
                        self.creation_lock.store(0, Ordering::Release);
                        wake_one_u32(&self.creation_lock);
                        return None;
                    }
                };

                // Register in all_slabs
                let mut current_all = self.all_slabs.load(Ordering::Relaxed);
                loop {
                    unsafe {
                        (*slab).next_all.store(current_all as usize, Ordering::Relaxed);
                    }
                    match self.all_slabs.compare_exchange(
                        current_all,
                        slab,
                        Ordering::Release,
                        Ordering::Relaxed
                    ) {
                        Ok(_) => break,
                        Err(actual) => current_all = actual,
                    }
                }
                
                // We have a new slab. Push to available and let loop handle it.
                // This is simpler than trying to install it directly and handling races again.
                unsafe { self.available.push(token, core::ptr::NonNull::new_unchecked(slab as *mut u8)); }
                
                self.creation_lock.store(0, Ordering::Release);
                wake_one_u32(&self.creation_lock);
            } else {
                // Failed to acquire lock. Wait.
                wait_on_u32(&self.creation_lock, 1);
            }
        }
    }

    pub unsafe fn free(&self, token: &impl GhostBorrow<'brand>, ptr: *mut u8) {
        // Find slab
        let slab_ptr = SegregatedSlab::<'brand, SIZE, N>::from_ptr(ptr);
        let slab = slab_ptr.as_ref();

        let prev_count = slab.free(token, ptr);

        if prev_count == N {
            // Transitioned from Full (N) to Available (N-1).
            // We are the thread that broke the fullness.
            self.available.push(token, core::ptr::NonNull::new_unchecked(slab_ptr.as_ptr() as *mut u8));
        }
    }

    pub fn free_batch(&self, token: &impl GhostBorrow<'brand>, batch: impl Iterator<Item = *mut u8>) {
        for ptr in batch {
            unsafe { self.free(token, ptr); }
        }
    }

    pub fn alloc_batch_into(&self, token: &impl GhostBorrow<'brand>, count: usize, buf: &mut Vec<*mut u8>) {
         // Try bump batch from active
         let active_ptr = self.active.load(Ordering::Acquire);
         if !active_ptr.is_null() {
             let slab = unsafe { &*active_ptr };
             if let Some(base) = slab.alloc_bump_batch(token, count) {
                 for i in 0..count {
                     unsafe { buf.push(base.add(i * SIZE)); }
                 }
                 return;
             }
         }

         // Fallback: loop alloc
         for _ in 0..count {
             if let Some(p) = self.alloc(token) {
                 buf.push(p);
             } else {
                 break;
             }
         }
    }
}

impl<'brand, SC: SizeClass, PA: PageAlloc + Default, const SIZE: usize, const N: usize> Drop for SizeClassManager<'brand, SC, PA, SIZE, N> {
    fn drop(&mut self) {
        // We only need to iterate all_slabs and free them.
        let mut current = self.all_slabs.load(Ordering::Relaxed);
        let layout = unsafe { Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE) };

        while !current.is_null() {
            unsafe {
                let slab = &*current;
                let next_val = slab.next_all.load(Ordering::Relaxed);

                // Drop slab
                ptr::drop_in_place(current);
                PA::default().dealloc_page(current as *mut u8, layout);

                current = next_val as *mut SegregatedSlab<'brand, SIZE, N>;
            }
        }
    }
}

/// A thread-local cache for a specific size class.
pub struct ThreadLocalCache<'brand, SC: SizeClass> {
    buffer: Vec<*mut u8>,
    _marker: PhantomData<SC>,
    _brand: PhantomData<fn(&GhostToken<'brand>)>,
}

impl<'brand, SC: SizeClass> ThreadLocalCache<'brand, SC> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            _marker: PhantomData,
            _brand: PhantomData,
        }
    }

    pub fn push(&mut self, ptr: *mut u8) {
         self.buffer.push(ptr);
    }

    pub fn pop(&mut self) -> Option<*mut u8> {
        self.buffer.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    pub fn flush<PA: PageAlloc + Default, const SIZE: usize, const N: usize>(&mut self, manager: &SizeClassManager<'brand, SC, PA, SIZE, N>, token: &impl GhostBorrow<'brand>) {
         if self.buffer.is_empty() { return; }
         manager.free_batch(token, self.buffer.drain(..));
    }

    pub fn fill<PA: PageAlloc + Default, const SIZE: usize, const N: usize>(&mut self, manager: &SizeClassManager<'brand, SC, PA, SIZE, N>, token: &impl GhostBorrow<'brand>, count: usize) {
        manager.alloc_batch_into(token, count, &mut self.buffer);
    }
}
