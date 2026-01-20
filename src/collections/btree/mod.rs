//! Branded B-Tree collections.
//!
//! This module provides B-Tree based maps and sets that are integrated with the
//! `halo` ghost token system. They use `BrandedVec` as a backing arena for nodes,
//! providing better cache locality than pointer-based trees and enabling
//! safe interior mutability via the token.

pub mod active;
pub mod active_bplus_tree;
pub mod bplus_tree;
pub mod btree_map;
pub mod btree_set;

pub use active::{ActivateBTreeMap, ActivateBTreeSet, ActiveBTreeMap, ActiveBTreeSet};
pub use btree_map::BrandedBTreeMap;
pub use btree_set::BrandedBTreeSet;
