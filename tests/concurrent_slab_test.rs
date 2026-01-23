use halo::{GhostToken, SharedGhostToken};
use halo::alloc::{ConcurrentBrandedSlab, ConcurrentGhostAlloc};
use core::alloc::Layout;
use std::thread;

#[test]
fn test_concurrent_basic() {
    GhostToken::new(|token| {
        let slab = ConcurrentBrandedSlab::new();
        let layout = Layout::new::<u64>();

        let ptr1 = slab.allocate(&token, layout).unwrap();
        unsafe {
            *(ptr1.as_ptr() as *mut u64) = 123;
            assert_eq!(*(ptr1.as_ptr() as *mut u64), 123);
            slab.deallocate(&token, ptr1, layout);
        }
    });
}

#[test]
fn test_concurrent_growth() {
    GhostToken::new(|token| {
        let slab = ConcurrentBrandedSlab::new();
        let layout = Layout::new::<u64>();

        // Page size is 4096. Block size for u64 (8 bytes) is 8.
        // Capacity per page ~ 500 blocks.
        // Allocate 1000 blocks to force growth.
        let mut ptrs = Vec::new();
        for i in 0..1000 {
            let ptr = slab.allocate(&token, layout).unwrap();
            unsafe { *(ptr.as_ptr() as *mut u64) = i as u64; }
            ptrs.push(ptr);
        }

        for (i, ptr) in ptrs.iter().enumerate() {
            unsafe {
                assert_eq!(*(ptr.as_ptr() as *mut u64), i as u64);
            }
        }

        for ptr in ptrs {
            unsafe { slab.deallocate(&token, ptr, layout); }
        }
    });
}

#[test]
fn test_concurrent_threads() {
    GhostToken::new(|token| {
        let slab = ConcurrentBrandedSlab::new();
        let shared_token = SharedGhostToken::new(token);
        let slab_ref = &slab;
        let token_ref = &shared_token;

        thread::scope(|s| {
            for t_idx in 0..4 {
                s.spawn(move || {
                    let guard = token_ref.read();
                    let layout = Layout::new::<u64>();

                    for i in 0..100 {
                        let ptr = slab_ref.allocate(&guard, layout).unwrap();
                        unsafe {
                            // Verify unique access by writing a value that depends on thread + iter
                            // and checking it briefly (though race on memory content is not prevented by slab itself,
                            // as alloc returns unique ptr).
                            // Since we have unique ptr, we are safe.
                            let val = (t_idx * 1000 + i) as u64;
                            *(ptr.as_ptr() as *mut u64) = val;

                            // Simulate some work
                            std::hint::spin_loop();

                            assert_eq!(*(ptr.as_ptr() as *mut u64), val);

                            slab_ref.deallocate(&guard, ptr, layout);
                        }
                    }
                });
            }
        });
    });
}

#[test]
fn test_concurrent_threads_growth() {
    GhostToken::new(|token| {
        let slab = ConcurrentBrandedSlab::new();
        let shared_token = SharedGhostToken::new(token);
        let slab_ref = &slab;
        let token_ref = &shared_token;

        thread::scope(|s| {
            for t_idx in 0..8 {
                s.spawn(move || {
                    let guard = token_ref.read();
                    let layout = Layout::new::<u64>();

                    // Allocate enough to trigger growth across threads
                    let mut ptrs = Vec::new();
                    for i in 0..200 {
                        let ptr = slab_ref.allocate(&guard, layout).unwrap();
                        unsafe {
                            let val = (t_idx * 1000 + i) as u64;
                            *(ptr.as_ptr() as *mut u64) = val;
                        }
                        ptrs.push(ptr);
                    }

                    for ptr in ptrs {
                        unsafe { slab_ref.deallocate(&guard, ptr, layout); }
                    }
                });
            }
        });
    });
}
