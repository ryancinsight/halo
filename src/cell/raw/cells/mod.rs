//! Raw cell implementations organized by functionality.
//!
//! This module contains the fundamental building blocks for ghost cells,
//! organized by their core functionality and safety properties.

pub mod cell;
pub mod ref_cell;
pub mod unsafe_cell;

pub use cell::GhostCell;
pub use ref_cell::GhostRefCell;
pub use unsafe_cell::GhostUnsafeCell;
