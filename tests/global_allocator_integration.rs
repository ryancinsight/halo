use halo::alloc::{DispatchGlobalAlloc, with_global_allocator, BrandedSlab};
use halo::GhostToken;

#[global_allocator]
static GLOBAL: DispatchGlobalAlloc = DispatchGlobalAlloc;

#[test]
fn test_global_allocator_dispatch() {
    // Before scope: should use System (default fallback)
    let v1: Vec<i32> = vec![1, 2, 3];
    assert_eq!(v1, vec![1, 2, 3]);

    GhostToken::new(|token| {
        let slab = BrandedSlab::new();

        unsafe {
            with_global_allocator(&slab, &token, || {
                // Inside scope: allocs should go to slab
                // Allocation 1: Vector
                let mut v2: Vec<i32> = Vec::with_capacity(10);
                v2.push(42);
                v2.push(100);

                assert_eq!(v2, vec![42, 100]);

                // Allocation 2: Box
                let b = Box::new(12345u64);
                assert_eq!(*b, 12345);

                // Allocation 3: Recursion stress test
                // Allocate enough objects to trigger new page allocations in BrandedSlab.
                // BrandedSlab needs to ask GlobalAlloc for pages.
                // DispatchGlobalAlloc should detect recursion and route page requests to System.

                let mut many_boxes = Vec::new();
                for i in 0..2000 {
                    many_boxes.push(Box::new(i));
                }

                for (i, b) in many_boxes.iter().enumerate() {
                    assert_eq!(**b, i);
                }

                // Explicitly drop inside scope to test dealloc dispatch
                drop(many_boxes);
                drop(b);
                drop(v2);
            });
        }

        // After scope: back to System
        let v3: Vec<i32> = vec![7, 8, 9];
        assert_eq!(v3, vec![7, 8, 9]);
    });
}
