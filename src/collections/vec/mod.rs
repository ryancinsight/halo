//! Vector-based collections optimized for Ghost-style usage.
//!
//! This module contains vector and vector-like data structures that are
//! branded for safe concurrent access patterns.

pub mod base_chunked_vec;
pub mod chunked_vec;
pub mod vec;
pub mod vec_deque;
pub mod small_vec;
pub mod slice;
pub mod active;

pub use base_chunked_vec::ChunkedVec;
pub use chunked_vec::BrandedChunkedVec;
pub use vec::BrandedVec;
pub use vec_deque::BrandedVecDeque;
pub use small_vec::BrandedSmallVec;
pub use slice::{BrandedSlice, BrandedSliceMut};
pub use active::{ActiveVec, ActivateVec};
