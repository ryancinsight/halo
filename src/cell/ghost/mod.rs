//! Safe token-gated cells (stratified implementation).
//!
//! Public surface is re-exported from `ghost_cell`, but the implementation is
//! split across small submodules to keep files short and responsibilities clear.

pub mod ghost_cell;

#[path = "ops/borrow.rs"]
mod ops_borrow;
#[path = "ops/copy.rs"]
mod ops_copy;
#[path = "ops/functional.rs"]
mod ops_functional;

pub use ghost_cell::GhostCell;
