//! Raw, token-branded building blocks.
//!
//! This layer intentionally exposes *minimal* surface area and concentrates
//! unsafe code in a small number of modules.

pub mod ghost_unsafe_cell;
pub mod ghost_cell;
pub mod ghost_ref_cell;

pub use ghost_unsafe_cell::GhostUnsafeCell;
pub use ghost_cell::GhostCell;
pub use ghost_ref_cell::GhostRefCell;







