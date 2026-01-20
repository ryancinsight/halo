//! Other collections optimized for Ghost-style usage.
//!
//! This module contains specialized collections like deques and arenas
//! that are branded for safe concurrent access patterns.

pub mod active;
pub mod binary_heap;
pub mod cow_strings;
pub mod deque;
pub mod disjoint_set;
pub mod doubly_linked_list;
pub mod fenwick_tree;
pub mod interval_map;
pub mod lru_cache;
pub mod segment_tree;
pub mod slot_map;
pub mod tripod_list;

pub use binary_heap::BrandedBinaryHeap;
pub use cow_strings::BrandedCowStrings;
pub use deque::BrandedDeque;
pub use disjoint_set::{ActiveDisjointSet, BrandedDisjointSet};
pub use doubly_linked_list::BrandedDoublyLinkedList;
pub use fenwick_tree::BrandedFenwickTree;
pub use interval_map::BrandedIntervalMap;
pub use lru_cache::BrandedLruCache;
pub use segment_tree::{BrandedSegmentTree, BrandedSegmentTreeViewMut};
pub use slot_map::{BrandedSlotMap, SlotKey};
pub use tripod_list::TripodList;
