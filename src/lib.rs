//! # `halo` - Protective Data Structure Toolkit
//!
//! A protective toolkit for safe, high-performance data structures and concurrency
//! primitives using ghost tokens. Provides memory-efficient interior mutability
//! with a subtle protective layer of type safety.
//!
//! ## Key Features
//!
//! - **Ghost token protection**: Linear ghost tokens create protective access boundaries
//! - **Zero-cost safety**: Safe access with no runtime borrow checking overhead
//! - **Comprehensive toolkit**: Cells, collections, concurrency, and graph primitives
//! - **Stratified design**: Foundation primitives → ergonomic APIs → domain-specific types
//!
//! ## Architecture
//!
//! Uses ghost tokens (branded phantom types + rank-2 polymorphism) to create
//! protective type boundaries, enabling safe interior mutability and concurrency patterns
//! without sacrificing performance or ergonomics.
//!
//! ## Example
//!
//! ```rust
//! use halo::{GhostToken, GhostCell};
//!
//! // Create a protective token boundary
//! GhostToken::new(|mut token| {
//!     let cell = GhostCell::new(42);
//!
//!     // Borrow mutably through the token
//!     let value = cell.borrow_mut(&mut token);
//!     *value = 100;
//!
//!     // Borrow immutably
//!     let borrowed = cell.borrow(&token);
//!     assert_eq!(*borrowed, 100);
//! });
//! ```

#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod cell;
pub mod collections;
pub mod concurrency;
pub mod graph;
pub mod token;

pub use cell::{GhostCell, GhostLazyCell, GhostLazyLock, GhostOnceCell, GhostUnsafeCell};
pub use collections::BrandedVec;
pub use graph::{GhostAdjacencyGraph, GhostBipartiteGraph, GhostCscGraph, GhostCsrGraph, GhostDag};
pub use token::GhostToken;

// Note: std::cell::Cell is not re-exported to maintain naming consistency
// Use GhostCell for the halo ecosystem, or import std::cell::Cell directly

// Compile-time assertions for memory layout optimizations
const _: () = {
    use core::mem;

    // Tokens are ZSTs.
    assert!(mem::size_of::<GhostToken<'static>>() == 0);

    // Foundational “zero-overhead” layout claims.
    //
    // `GhostUnsafeCell` is `repr(transparent)` over `UnsafeCell<T>` (brand is a ZST),
    // therefore it must match size + alignment exactly.
    assert!(
        mem::size_of::<GhostUnsafeCell<'static, i32>>() == mem::size_of::<core::cell::UnsafeCell<i32>>()
    );
    assert!(
        mem::align_of::<GhostUnsafeCell<'static, i32>>() == mem::align_of::<core::cell::UnsafeCell<i32>>()
    );

    // `GhostCell` must remain a thin wrapper around the raw cell.
    assert!(mem::size_of::<GhostCell<'static, i32>>() == mem::size_of::<GhostUnsafeCell<'static, i32>>());
    assert!(mem::align_of::<GhostCell<'static, i32>>() == mem::align_of::<GhostUnsafeCell<'static, i32>>());

    // Lazy/once primitives should remain small and allocation-free (struct size).
    // These are intentionally loose upper bounds to avoid platform brittleness,
    // while still catching accidental large regressions.
    assert!(mem::size_of::<GhostOnceCell<'static, u64>>() <= mem::size_of::<usize>() * 4);
    assert!(mem::size_of::<GhostLazyCell<'static, u64>>() <= mem::size_of::<usize>() * 6);
    assert!(mem::size_of::<GhostLazyLock<'static, u64>>() <= mem::size_of::<usize>() * 6);
};
