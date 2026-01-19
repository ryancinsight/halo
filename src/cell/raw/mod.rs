//! Raw, token-branded building blocks.
//!
//! This layer intentionally exposes *minimal* surface area and concentrates
//! unsafe code in a small number of modules. Cell implementations are organized
//! by their core functionality in the `cells` submodule.

pub(crate) mod access;
pub mod cells;

pub use cells::{GhostCell, GhostRefCell, GhostUnsafeCell};
