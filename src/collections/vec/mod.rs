//! Vector-based collections optimized for Ghost-style usage.
//!
//! This module contains vector and vector-like data structures that are
//! branded for safe concurrent access patterns.

pub mod active;
pub mod base_chunked_vec;
pub mod chunked_vec;
pub mod matrix;
pub mod slice;
pub mod small_vec;
pub mod vec;
pub mod vec_deque;

pub use active::{ActivateVec, ActiveVec};
pub use base_chunked_vec::ChunkedVec;
pub use chunked_vec::BrandedChunkedVec;
pub use matrix::{BrandedMatrix, BrandedMatrixViewMut};
pub use slice::{BrandedSlice, BrandedSliceMut};
pub use small_vec::BrandedSmallVec;
pub use vec::{BrandedArray, BrandedVec};
pub use vec_deque::BrandedVecDeque;
