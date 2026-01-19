//! Collections optimized for Ghost-style usage.
//!
//! Collections are organized by data structure type:
//! - `vec`: Vector and vector-like collections
//! - `hash`: Hash-based collections (maps and sets)
//! - `btree`: B-Tree based collections (maps and sets)
//! - `other`: Specialized collections (deques, arenas)

pub mod vec;
pub mod hash;
pub mod btree;
pub mod trie;
pub mod other;
pub mod string;
pub mod skip_list;

// Re-export commonly used types from submodules
pub use vec::{
    BrandedVec, BrandedArray, BrandedVecDeque, BrandedChunkedVec, ChunkedVec, BrandedSmallVec,
    ActiveVec, ActivateVec, BrandedSlice, BrandedSliceMut, BrandedMatrix, BrandedMatrixViewMut
};
pub use hash::{
    BrandedHashMap, BrandedHashSet, BrandedIndexMap,
    ActiveHashMap, ActivateHashMap, ActiveHashSet, ActivateHashSet
};
pub use btree::{BrandedBTreeMap, BrandedBTreeSet};
pub use trie::{BrandedRadixTrieMap, BrandedRadixTrieSet};
pub use other::{
    BrandedBinaryHeap, BrandedCowStrings, BrandedDeque, BrandedDoublyLinkedList,
    BrandedLruCache, BrandedSlotMap, SlotKey, BrandedIntervalMap,
    BrandedSegmentTree, BrandedSegmentTreeViewMut,
};
pub use skip_list::{BrandedSkipList, ActiveSkipList, ActivateSkipList};

pub use crate::alloc::BrandedArena;
pub use string::{BrandedString, ActiveString, ActivateString};

// Re-export for trait definitions
pub use crate::GhostToken;

/// Zero-cost abstraction trait for branded collections.
/// Provides common operations with guaranteed zero runtime overhead.
pub trait BrandedCollection<'brand> {
    /// Returns true if the collection is empty.
    fn is_empty(&self) -> bool;

    /// Returns the number of elements in the collection.
    fn len(&self) -> usize;
}

/// Extension trait for zero-copy operations on single-item branded collections.
pub trait ZeroCopyOps<'brand, T> {
    /// Zero-copy find operation.
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool;

    /// Zero-copy any operation with short-circuiting.
    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool;

    /// Zero-copy all operation with short-circuiting.
    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool;
}

/// Extension trait for zero-copy operations on key-value branded collections.
pub trait ZeroCopyMapOps<'brand, K, V> {
    /// Zero-copy find operation on key-value pairs.
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<(&'a K, &'a V)>
    where
        F: Fn(&K, &V) -> bool;

    /// Zero-copy any operation with short-circuiting.
    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool;

    /// Zero-copy all operation with short-circuiting.
    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&K, &V) -> bool;
}
