//! Traits for abstracting over different kinds of ghost tokens.

use crate::token::GhostToken;

/// A trait for tokens that can authorize shared access (borrowing).
///
/// This is implemented by `GhostToken`, `HierarchicalGhostToken` (with read permission),
/// and `ImmutableChild`.
pub trait GhostBorrow<'brand> {}

/// A trait for tokens that can authorize exclusive access (mutable borrowing).
///
/// This is implemented by `GhostToken` and `HierarchicalGhostToken` (with full permission).
pub trait GhostBorrowMut<'brand>: GhostBorrow<'brand> {}

// Implement for standard GhostToken
impl<'brand> GhostBorrow<'brand> for GhostToken<'brand> {}
impl<'brand> GhostBorrowMut<'brand> for GhostToken<'brand> {}

// We will implement for Hierarchical tokens in hierarchy.rs
