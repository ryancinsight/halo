use core::alloc::Layout;
use core::ptr::NonNull;
use std::alloc::{alloc, dealloc, Layout as StdLayout};

// 16 bytes min block size
pub const MIN_BLOCK_SIZE: usize = 16;
// 16MB heap size
pub const HEAP_SIZE: usize = 16 * 1024 * 1024;
// Min order size 16 = 2^4. 16MB = 2^24.
// Orders range from 0 (16 bytes) to 20 (16MB).
pub const MAX_ORDER: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum NodeState {
    Unused = 0, // Free and in free list
    Used = 1,   // Allocated
    Split = 2,  // Split into children
}

#[repr(C)]
struct FreeBlock {
    next: Option<NonNull<FreeBlock>>,
    prev: Option<NonNull<FreeBlock>>,
}

pub struct BuddyAllocator {
    base: NonNull<u8>,
    layout: StdLayout,
    tree: Vec<u8>, // Implicit binary tree state
    free_lists: [Option<NonNull<FreeBlock>>; MAX_ORDER + 1],
}

// Send is safe because the allocator owns its memory and is protected by Mutex in BrandedHeap.
unsafe impl Send for BuddyAllocator {}

impl BuddyAllocator {
    pub fn new(size: usize) -> Option<Self> {
        if size != HEAP_SIZE {
            return None;
        }

        let layout = StdLayout::from_size_align(size, 4096).ok()?;
        let ptr = unsafe { alloc(layout) };
        let base = NonNull::new(ptr)?;

        // Number of leaf blocks = HEAP_SIZE / MIN_BLOCK_SIZE
        // Tree size = 2 * leaf_count - 1
        // leaf_count = 2^20 = 1,048,576
        // tree_size = 2,097,151
        let leaf_count = HEAP_SIZE / MIN_BLOCK_SIZE;
        let tree_size = 2 * leaf_count;
        let tree = vec![NodeState::Unused as u8; tree_size];

        let mut allocator = BuddyAllocator {
            base,
            layout,
            tree,
            free_lists: [None; MAX_ORDER + 1],
        };

        unsafe {
            allocator.push_free_block(base.as_ptr() as *mut FreeBlock, MAX_ORDER);
        }

        Some(allocator)
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE);
        let order = self.size_to_order(size);

        // Find smallest available block with order >= requested
        for i in order..=MAX_ORDER {
            if self.free_lists[i].is_some() {
                // Found a block. Split it down to 'order'.
                let ptr = unsafe { self.pop_free_block(i).unwrap() };
                let block_ptr = ptr.as_ptr();

                // Split loop
                let mut current_order = i;
                let mut current_ptr = block_ptr;

                while current_order > order {
                    // Split
                    let next_order = current_order - 1;
                    // Mark parent as split
                    let parent_idx = self.get_tree_index(self.ptr_to_offset(current_ptr as *const u8), current_order);
                    if parent_idx < self.tree.len() {
                        self.tree[parent_idx] = NodeState::Split as u8;
                    }

                    // Current block becomes left child
                    // Buddy is right child
                    let buddy_ptr = unsafe { current_ptr.add(1 << (next_order + 4)) }; // +4 because order 0 is 16 bytes (2^4)

                    // Add buddy to free list
                    unsafe { self.push_free_block(buddy_ptr as *mut FreeBlock, next_order); }

                    // Continue with left child
                    current_order = next_order;
                }

                // Mark the final block as Used
                let idx = self.get_tree_index(self.ptr_to_offset(current_ptr as *const u8), order);
                if idx < self.tree.len() {
                     self.tree[idx] = NodeState::Used as u8;
                }

                return NonNull::new(current_ptr as *mut u8);
            }
        }

        None
    }

    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE);
        let mut order = self.size_to_order(size);
        let mut offset = self.ptr_to_offset(ptr.as_ptr());

        // Mark as unused
        let mut idx = self.get_tree_index(offset, order);
        if idx < self.tree.len() {
            self.tree[idx] = NodeState::Unused as u8;
        }

        // Merge loop
        while order < MAX_ORDER {
            let buddy_offset = offset ^ (1 << (order + 4)); // Buddy offset calculation

            // Check if buddy is out of bounds (can happen for last blocks in non-pow2, but here we enforce pow2)
            if buddy_offset >= HEAP_SIZE {
                break;
            }

            let buddy_idx = self.get_tree_index(buddy_offset, order);

            // Check if buddy is free (Unused)
            // Note: If buddy is Split or Used, we can't merge.
            // Also need to check if buddy is in bounds.
             if buddy_idx >= self.tree.len() || self.tree[buddy_idx] != NodeState::Unused as u8 {
                break;
            }

            // Buddy is free. Remove buddy from list.
            let buddy_ptr = self.base.as_ptr().add(buddy_offset);

            // IMPORTANT: If we are merging, we MUST remove the buddy from the free list.
            // But remove_from_list is generic. We need to know if the buddy is actually IN the list.
            // The tree says it is Unused, so it SHOULD be in the list for 'order'.
            self.remove_from_list(buddy_ptr as *mut FreeBlock, order);

            // Move to parent
            // Parent offset is min(offset, buddy_offset)
            offset = offset & buddy_offset; // standard buddy logic
            order += 1;

            // Update parent state to Unused (it was Split)
            idx = self.get_tree_index(offset, order);
            if idx < self.tree.len() {
                self.tree[idx] = NodeState::Unused as u8;
            }
        }

        // Add the coalesced block to free list
        let block_ptr = self.base.as_ptr().add(offset);
        self.push_free_block(block_ptr as *mut FreeBlock, order);
    }

    fn size_to_order(&self, size: usize) -> usize {
        // order 0 = 16 (2^4)
        // order k = 16 * 2^k
        if size <= MIN_BLOCK_SIZE {
            return 0;
        }
        let mut s = size;

        // Ceil power of 2
        s = s.next_power_of_two();

        let zeros = s.trailing_zeros(); // log2(s)
        // 16 is 2^4. So order = zeros - 4.
        if zeros < 4 { 0 } else { (zeros - 4) as usize }
    }

    fn ptr_to_offset(&self, ptr: *const u8) -> usize {
        unsafe { ptr.offset_from(self.base.as_ptr()) as usize }
    }

    // 1-based index for heap array
    // Level 0 is root (order MAX_ORDER). Index 1.
    // Level k has 2^k nodes.
    // Here 'order' is the block order (0 is leaves).
    // So depth = MAX_ORDER - order.
    fn get_tree_index(&self, offset: usize, order: usize) -> usize {
        // level = MAX_ORDER - order
        // nodes_before_level = 2^level - 1
        // index_in_level = offset / block_size = offset >> (order + 4)
        // index = nodes_before_level + index_in_level
        // But usually root is 1.
        // Let's use 1-based indexing.

        let level = MAX_ORDER - order;
        let index_in_level = offset >> (order + 4);

        // Correct level calculation?
        // level 0 (root): 2^0 = 1 node. index 1.
        // level 1: 2^1 = 2 nodes. indices 2, 3.
        // starting index of level L is 2^L.

        (1 << level) + index_in_level - 1
    }

    // Safety: ptr must be valid and aligned.
    unsafe fn push_free_block(&mut self, ptr: *mut FreeBlock, order: usize) {
        let node = &mut *ptr;
        node.prev = None;
        node.next = self.free_lists[order];

        if let Some(mut head) = self.free_lists[order] {
            head.as_mut().prev = NonNull::new(ptr);
        }
        self.free_lists[order] = NonNull::new(ptr);
    }

    unsafe fn pop_free_block(&mut self, order: usize) -> Option<NonNull<FreeBlock>> {
        let head_ptr = self.free_lists[order]?;
        let head = head_ptr.as_ptr();

        let next = (*head).next;
        if let Some(mut next_ptr) = next {
            next_ptr.as_mut().prev = None;
        }
        self.free_lists[order] = next;

        // Clear links in popped block to prevent corruption?
        // Not strictly necessary but good for debug.
        (*head).next = None;
        (*head).prev = None;

        Some(head_ptr)
    }

    unsafe fn remove_from_list(&mut self, ptr: *mut FreeBlock, order: usize) {
        let node = &mut *ptr;

        let prev = node.prev;
        let next = node.next;

        if let Some(mut prev_ptr) = prev {
            prev_ptr.as_mut().next = next;
        } else {
            // It was head
            if self.free_lists[order] == NonNull::new(ptr) {
                 self.free_lists[order] = next;
            } else {
                // This shouldn't happen if prev is None but it wasn't head?
                // Actually, if prev is None, it MUST be head.
                // If it's not head, prev MUST NOT be None.
                // Unless logic error elsewhere.
            }
        }

        if let Some(mut next_ptr) = next {
            next_ptr.as_mut().prev = prev;
        }

        node.next = None;
        node.prev = None;
    }
}

impl Drop for BuddyAllocator {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.base.as_ptr(), self.layout);
        }
    }
}
