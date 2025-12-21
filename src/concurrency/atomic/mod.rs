//! Branded, lock-free atomic primitives.
//!
//! These types provide **concurrent writer** access using hardware atomics while
//! keeping the *ghost/brand* aspect purely compile-time (zero runtime borrow state).
//!
//! Important:
//! - This does **not** make concurrent mutation “free”. Atomic RMW operations have
//!   inherent hardware cost.
//! - The goal is that the *wrapper* overhead is optimized away.

/// Branded `AtomicBool`.
pub mod bool;
/// Branded `AtomicU64`.
pub mod u64;
/// Branded `AtomicUsize`.
pub mod usize;
/// Branded atomic bitsets.
pub mod bitset;

pub use bool::GhostAtomicBool;
pub use u64::GhostAtomicU64;
pub use usize::GhostAtomicUsize;
pub use bitset::GhostAtomicBitset;


