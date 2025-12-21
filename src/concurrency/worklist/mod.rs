//! Lock-free worklists for parallel algorithms.
//!
//! This module focuses on minimal, branded building blocks that compose with the
//! Ghost-style ecosystem (brand is compile-time only, overhead should optimize away).

pub mod treiber_stack;
pub mod chase_lev_deque;

pub use treiber_stack::GhostTreiberStack;
pub use chase_lev_deque::GhostChaseLevDeque;


