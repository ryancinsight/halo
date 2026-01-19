//! Internal graph accessors and low-level building blocks.
//!
//! This module is intentionally `pub(crate)` so graph implementations can share
//! fast, branded primitives (visited sets, scratch buffers, etc.) without
//! exposing them as part of the public API surface.

pub(crate) mod visited;
