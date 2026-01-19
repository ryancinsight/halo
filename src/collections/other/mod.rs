//! Other collections optimized for Ghost-style usage.
//!
//! This module contains specialized collections like deques and arenas
//! that are branded for safe concurrent access patterns.

pub mod deque;
pub mod cow_strings;
pub mod doubly_linked_list;
pub mod binary_heap;
pub mod lru_cache;
pub mod slot_map;
pub mod interval_map;
pub mod segment_tree;
pub mod active;

pub use deque::BrandedDeque;
pub use cow_strings::BrandedCowStrings;
pub use doubly_linked_list::BrandedDoublyLinkedList;
pub use binary_heap::BrandedBinaryHeap;
pub use lru_cache::BrandedLruCache;
pub use slot_map::{BrandedSlotMap, SlotKey};
pub use interval_map::BrandedIntervalMap;
pub use segment_tree::{BrandedSegmentTree, BrandedSegmentTreeViewMut};
