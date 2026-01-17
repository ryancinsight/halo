//! Other collections optimized for Ghost-style usage.
//!
//! This module contains specialized collections like deques and arenas
//! that are branded for safe concurrent access patterns.

pub mod deque;
pub mod arena;
pub mod cow_strings;
pub mod doubly_linked_list;
pub mod binary_heap;
pub mod lru_cache;

pub use deque::BrandedDeque;
pub use arena::BrandedArena;
pub use cow_strings::BrandedCowStrings;
pub use doubly_linked_list::BrandedDoublyLinkedList;
pub use binary_heap::BrandedBinaryHeap;
pub use lru_cache::BrandedLruCache;
