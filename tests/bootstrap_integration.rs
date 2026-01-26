use halo::allocator::bootstrap::init::bootstrap;
use halo::alloc::{BrandedSlab, init_slab_page, GhostAlloc};
use halo::GhostToken;
use std::alloc::Layout;
use std::ptr::NonNull;
use std::mem::ManuallyDrop;

#[test]
fn test_bootstrap_and_injection() {
    // 1. Bootstrap
    let arena = bootstrap().expect("Bootstrap failed");
    assert!(arena.capacity() >= 64 * 1024 * 1024);

    // 2. Carve a page
    let page_ptr_raw = arena.alloc_page().expect("Failed to alloc page from arena");
    let page_ptr = NonNull::new(page_ptr_raw).unwrap();

    // 3. Initialize page manually
    // Use block size 32 (must be power of 2 and >= 8)
    unsafe {
        let success = init_slab_page(page_ptr, 32, 0);
        assert!(success, "Failed to initialize slab page");
    }

    // 4. Create a BrandedSlab and inject the page
    GhostToken::new(|mut token| {
        // Use ManuallyDrop to prevent cleanup of arena page on panic
        // because BrandedSlab calls dealloc which assumes heap memory.
        let slab = ManuallyDrop::new(BrandedSlab::new());

        // Inject into shard 0
        unsafe {
            slab.inject_page(&mut token, page_ptr);
        }

        // 5. Allocate from the slab. Use allocate_in to force shard 0.
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = slab.allocate_in(&token, layout, Some(0)).expect("Allocation failed");

        let ptr_addr = ptr.as_ptr() as usize;
        let page_addr = page_ptr.as_ptr() as usize;

        assert!(ptr_addr >= page_addr && ptr_addr < page_addr + 4096,
            "Allocated pointer {:#x} should be in injected page {:#x}", ptr_addr, page_addr);
    });
}
