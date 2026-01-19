//! Ghost cell family - token-branded interior mutability primitives.
//!
//! The module tree is intentionally stratified:
//! - `raw::*` are the minimal unsafe building blocks.
//! - `ghost::*` are the safe, token-gated cell abstractions.
//! - `lazy::*` are initialization and memoization-style building blocks.

pub mod ghost;
pub mod lazy;
pub mod raw;

pub use ghost::GhostCell;
pub use lazy::{GhostLazyCell, GhostLazyLock, GhostOnceCell};
pub use raw::{GhostCell as RawGhostCell, GhostRefCell, GhostUnsafeCell};
