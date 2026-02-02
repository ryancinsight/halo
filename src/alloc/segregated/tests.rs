use crate::GhostToken;
use crate::alloc::segregated::size_class::{SC, get_size_class_index, get_block_size};
use crate::alloc::segregated::freelist::BrandedFreelist;
use crate::alloc::segregated::slab::SegregatedSlab;
use crate::alloc::segregated::manager::{SizeClassManager, ThreadLocalCache};
use crate::alloc::page::GlobalPageAlloc;
use core::ptr;
use core::ptr::NonNull;

#[test]
fn test_size_class_helpers() {
    assert_eq!(get_size_class_index(8), Some(0)); // 16
    assert_eq!(get_size_class_index(16), Some(0));
    assert_eq!(get_size_class_index(17), Some(1)); // 32
    assert_eq!(get_size_class_index(32), Some(1));
    assert_eq!(get_block_size(0), 16);
    assert_eq!(get_block_size(1), 32);
}

#[test]
fn test_freelist_basic() {
    GhostToken::new(|token| {
        let fl = BrandedFreelist::new();
        // Use stack variables as blocks
        let mut block1 = [0usize; 4];
        let mut block2 = [0usize; 4];
        let p1 = NonNull::new(block1.as_mut_ptr() as *mut u8).unwrap();
        let p2 = NonNull::new(block2.as_mut_ptr() as *mut u8).unwrap();

        unsafe {
            fl.push(&token, p1);
            fl.push(&token, p2);
            assert_eq!(fl.pop(&token), Some(p2));
            assert_eq!(fl.pop(&token), Some(p1));
            assert_eq!(fl.pop(&token), None);
        }
    });
}

#[test]
fn test_freelist_batch() {
    GhostToken::new(|token| {
        let fl = BrandedFreelist::new();
        let mut blocks = [[0usize; 4]; 5];
        let ptrs: Vec<_> = blocks
            .iter_mut()
            .map(|b| NonNull::new(b.as_mut_ptr() as *mut u8).unwrap())
            .collect();

        unsafe {
            fl.push_batch(&token, ptrs.clone());
            let popped = fl.pop_batch(&token, 5);
            assert_eq!(popped.len(), 5);
            // LIFO: Pushed 0..4 -> Popped 4..0
            assert_eq!(popped[0], ptrs[4]);
            assert_eq!(popped[4], ptrs[0]);
        }
    });
}

#[test]
fn test_slab_basic() {
    GhostToken::new(|token| {
        // Alloc 16 byte objects. Slab 4096.
        // N can be up to ~250.
        const N: usize = 100;
        let slab = SegregatedSlab::<'_, 16, N>::new().unwrap();
        let slab_ref = unsafe { slab.as_ref() };

        let p1 = slab_ref.alloc(&token).unwrap();
        let p2 = slab_ref.alloc(&token).unwrap();
        assert_ne!(p1, p2);

        unsafe { slab_ref.free(&token, p1); }
        let p3 = slab_ref.alloc(&token).unwrap();
        assert_eq!(p3, p1); // LIFO reuse

        // Fill slab
        let mut ptrs = Vec::new();
        ptrs.push(p2);
        ptrs.push(p3);

        for _ in 0..(N - 2) {
             ptrs.push(slab_ref.alloc(&token).unwrap());
        }
        assert!(slab_ref.alloc(&token).is_none()); // Full

        // Clean up manually
        unsafe { ptr::drop_in_place(slab.as_ptr()); }
    });
}

#[test]
fn test_manager_integration() {
    GhostToken::new(|token| {
        const N: usize = 64;
        let manager = SizeClassManager::<'_, SC<32>, GlobalPageAlloc, 32, N>::new();

        let mut ptrs = Vec::new();
        // Alloc more than one slab
        for _ in 0..N + 10 {
            if let Some(p) = manager.alloc(&token) {
                ptrs.push(p);
            }
        }
        assert_eq!(ptrs.len(), N + 10);

        // Free some
        for p in ptrs.drain(0..15) {
             unsafe { manager.free(&token, p); }
        }

        // Alloc again
        let p = manager.alloc(&token).unwrap();
        unsafe { manager.free(&token, p); }

        // Note: We leak the slabs here as SizeClassManager doesn't implement Drop.
    });
}

#[test]
fn test_thread_local_cache() {
    GhostToken::new(|token| {
        const N: usize = 64;
        let manager = SizeClassManager::<'_, SC<32>, GlobalPageAlloc, 32, N>::new();
        let mut cache = ThreadLocalCache::<'_, SC<32>>::new(10);

        // Fill
        cache.fill(&manager, &token, 5);
        assert!(!cache.is_empty());

        let p = cache.pop().unwrap();
        cache.push(p);

        // Flush
        cache.flush(&manager, &token);
        assert!(cache.is_empty());
    });
}
