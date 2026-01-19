//! Synchronization primitives.

pub mod mpmc;
pub use mpmc::GhostRingBuffer;
