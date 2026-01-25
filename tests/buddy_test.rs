#[cfg(test)]
mod tests {
    use halo::alloc::buddy::{BuddyAllocator, HEAP_SIZE, MAX_ORDER};
    use core::alloc::Layout;
    use core::ptr::NonNull;

    #[test]
    fn test_buddy_alloc_basic() {
        let mut allocator = BuddyAllocator::new(HEAP_SIZE).expect("Failed to create allocator");
        // Alloc and dealloc one small block
        let layout = Layout::from_size_align(64, 64).unwrap();
        let ptr = allocator.alloc(layout).unwrap();

        // This crashes:
        unsafe {
             allocator.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_tree_indexing() {
        // Test basic tree logic
        let mut allocator = BuddyAllocator::new(HEAP_SIZE).expect("Failed to create allocator");
    }
}
