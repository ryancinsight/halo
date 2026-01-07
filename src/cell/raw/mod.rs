//! Raw, token-branded building blocks.
//!
//! This layer intentionally exposes *minimal* surface area and concentrates
//! unsafe code in a small number of modules. Cell implementations are organized
//! by their core functionality in the `cells` submodule.

pub mod cells;
pub(crate) mod access;

pub use cells::{GhostUnsafeCell, GhostCell, GhostRefCell};







