//! Centralized unsafe accessors for the raw cell layer.
//!
//! This module exists to *concentrate* and *standardize* unsafe pointer and
//! initialization operations used by raw ghost-branded primitives.
//!
//! ## Design rule
//! - Higher layers (`cell::raw::cells::*`, `cell::ghost::*`, collections, graphs)
//!   should not perform ad-hoc `ptr::*` / `MaybeUninit` unsafe operations.
//! - Instead, they should call the small, audited surface here.
//!
//! ## Why this is safe (invariant framing)
//! This module does **not** make operations safe by itself. It provides *uniform*
//! building blocks whose safety conditions are documented and can be audited in one place.

pub(crate) mod maybe_uninit;
pub(crate) mod ghost_unsafe_cell;


