use halo::alloc::{BrandedBumpAllocator, GhostAlloc};
use halo::GhostToken;
use std::alloc::Layout;

#[test]
fn test_bump_allocator() {
    GhostToken::new(|mut token| {
        let allocator = BrandedBumpAllocator::new();

        let x = allocator.alloc(42);
        let y = allocator.alloc(100);
        let s = allocator.alloc_str("hello");

        assert_eq!(*x, 42);
        assert_eq!(*y, 100);
        assert_eq!(s, "hello");

        *x += 1;
        assert_eq!(*x, 43);

        // GhostCell allocation
        let c = allocator.alloc_cell(10);
        assert_eq!(*c.borrow(&token), 10);

        *c.borrow_mut(&mut token) = 20;
        assert_eq!(*c.borrow(&token), 20);

        // Trait usage
        let layout = Layout::new::<u64>();
        let ptr = allocator.allocate(layout).unwrap();
        unsafe {
            *(ptr.as_ptr() as *mut u64) = 999;
            assert_eq!(*(ptr.as_ptr() as *mut u64), 999);
        }
    });
}
