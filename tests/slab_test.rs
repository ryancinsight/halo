use halo::{GhostToken, SharedGhostToken};
use halo::alloc::{BrandedSlab, GhostAlloc};
use core::alloc::Layout;
use std::thread;

#[test]
fn test_exclusive_alloc() {
    GhostToken::new(|mut token| {
        let slab = BrandedSlab::new();
        let layout = Layout::new::<u64>();

        // Use GhostAlloc explicitly via shared reborrow
        let ptr1 = GhostAlloc::allocate(&slab, &token, layout).unwrap();
        let ptr2 = GhostAlloc::allocate(&slab, &token, layout).unwrap();

        unsafe {
            *(ptr1.as_ptr() as *mut u64) = 100;
            *(ptr2.as_ptr() as *mut u64) = 200;
            assert_eq!(*(ptr1.as_ptr() as *mut u64), 100);
            assert_eq!(*(ptr2.as_ptr() as *mut u64), 200);

            GhostAlloc::deallocate(&slab, &token, ptr1, layout);
            GhostAlloc::deallocate(&slab, &token, ptr2, layout);
        }

        // Use exclusive optimization inherent method
        let ptr3 = slab.allocate_mut(&mut token, layout).unwrap();
        unsafe {
            *(ptr3.as_ptr() as *mut u64) = 300;
            assert_eq!(*(ptr3.as_ptr() as *mut u64), 300);
            slab.deallocate_mut(&mut token, ptr3, layout);
        }
    });
}

#[test]
fn test_concurrent_alloc() {
    GhostToken::new(|token| {
        let slab = BrandedSlab::new();
        let shared_token = SharedGhostToken::new(token);
        let slab_ref = &slab;
        let token_ref = &shared_token;

        thread::scope(|s| {
            for t_idx in 0..4 {
                s.spawn(move || {
                    let guard = token_ref.read();
                    let layout = Layout::new::<u64>();

                    for i in 0..100 {
                        // Use GhostAlloc concurrent path
                        let ptr = GhostAlloc::allocate(slab_ref, &guard, layout).unwrap();
                        unsafe {
                            let val = (t_idx * 1000 + i) as u64;
                            *(ptr.as_ptr() as *mut u64) = val;
                            std::hint::spin_loop(); // small delay
                            assert_eq!(*(ptr.as_ptr() as *mut u64), val);

                            GhostAlloc::deallocate(slab_ref, &guard, ptr, layout);
                        }
                    }
                });
            }
        });
    });
}

#[test]
fn test_mixed_alloc_phases() {
    // Phase 1: Exclusive alloc (fast)
    // Phase 2: Concurrent alloc (slow)
    // Phase 3: Exclusive alloc again

    GhostToken::new(|mut token| {
        let slab = BrandedSlab::new();
        let layout = Layout::new::<u64>();

        // 1. Exclusive
        let mut ptrs = Vec::new();
        for _ in 0..10 {
            ptrs.push(slab.allocate_mut(&mut token, layout).unwrap());
        }

        // 2. Concurrent (simulate via scoping)
        token.with_scoped(|sub_token| {
            let shared = SharedGhostToken::new(sub_token);
            let guard = shared.read();
            for _ in 0..10 {
                // Allocate using GhostAlloc
                ptrs.push(GhostAlloc::allocate(&slab, &guard, layout).unwrap());
            }
        });

        // 3. Back to exclusive
        for _ in 0..10 {
            ptrs.push(slab.allocate_mut(&mut token, layout).unwrap());
        }

        // Cleanup all using exclusive optimization where possible
        for ptr in ptrs {
            unsafe { slab.deallocate_mut(&mut token, ptr, layout); }
        }
    });
}
