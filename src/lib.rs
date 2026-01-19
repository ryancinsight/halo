//! # `halo` - Protective Data Structure Toolkit
//!
//! A protective toolkit for safe, high-performance data structures and concurrency
//! primitives using ghost tokens. Provides memory-efficient interior mutability
//! with a subtle protective layer of type safety.
//!
//! ## Safety Guarantees
//!
//! ### Memory Safety
//! - **No unsafe code in public APIs**: All safety-critical operations are implemented
//!   using safe Rust abstractions built on top of carefully audited unsafe foundations.
//! - **Linear token capability**: Ghost tokens cannot be duplicated, ensuring exclusive
//!   access patterns prevent data races and use-after-free scenarios.
//! - **Branded types**: Compile-time separation of data domains prevents accidental
//!   mixing of incompatible state.
//!
//! ### Concurrency Safety
//! - **Lock-free algorithms**: Atomic operations provide wait-free progress guarantees
//!   for concurrent data structures.
//! - **Memory barriers**: Proper ordering constraints prevent reordering hazards
//!   in multi-threaded scenarios.
//! - **ABA prevention**: Tagged atomics and validation prevent ABA problems
//!   in lock-free data structures.
//!
//! ### Formal Verification
//! - **Mathematical invariants**: Runtime checking of graph theory and data structure
//!   properties in debug builds.
//! - **Compile-time assertions**: Const generics provide static verification of
//!   capacity and size constraints.
//! - **Theorem validation**: Key algorithms include formal correctness proofs.
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
//! ### Core Abstractions
//!
//! 1. **Ghost Tokens** (`GhostToken<'brand>`):
//!    - Zero-sized linear capabilities
//!    - Branded with lifetime parameters for type-level separation
//!    - Enforce exclusive access patterns at compile time
//!
//! 2. **Ghost Cells** (`GhostCell<'brand, T>`):
//!    - Safe interior mutability through token gating
//!    - No runtime borrow checking overhead
//!    - Compile-time aliasing control
//!
//! 3. **Branded Collections** (`BrandedVec<'brand, T>`, etc.):
//!    - Token-gated access to entire collections
//!    - Bulk operations with guaranteed consistency
//!    - Zero-cost abstraction over standard collections
//!
//! 4. **Concurrent Primitives** (`GhostAtomicUsize`, `GhostTreiberStack`, etc.):
//!    - Lock-free data structures with formal correctness
//!    - Memory-safe concurrent algorithms
//!    - Progress guarantees (wait-free, lock-free)
//!
//! ### Safety Proofs
//!
//! The library's safety relies on several key theorems:
//!
//! **Theorem 1 (Token Linearity)**: A `GhostToken<'brand>` cannot be duplicated,
//! ensuring that at most one mutable reference to branded data exists at any time.
//!
//! **Theorem 2 (Branded Separation)**: Data branded with different `'brand` lifetimes
//! cannot be accessed with incompatible tokens, preventing type confusion.
//!
//! **Theorem 3 (Atomic Correctness)**: Lock-free algorithms maintain linearizability
//! and progress guarantees under the C++20 memory model assumptions.
//!
//! **Theorem 4 (Graph Invariants)**: DAG operations maintain topological properties
//! and provide cycle detection with mathematical guarantees.
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

pub mod alloc;
pub mod cell;
pub mod collections;
pub mod concurrency;
pub mod graph;
pub mod token;

pub use alloc::BrandedArena;
pub use cell::{GhostCell, GhostLazyCell, GhostLazyLock, GhostOnceCell, GhostUnsafeCell, RawGhostCell, GhostRefCell};
pub use collections::{
    BrandedVec,
    BrandedArray,
    BrandedVecDeque,
    BrandedHashMap,
    BrandedHashSet,
    BrandedCowStrings,
    BrandedString,
    BrandedDoublyLinkedList,
    BrandedIntervalMap,
    ActivateVec,
    ActiveVec,
    BrandedSlice,
    BrandedSliceMut,
    BrandedMatrix,
    BrandedMatrixViewMut,
};
pub use graph::{GhostAdjacencyGraph, GhostBipartiteGraph, GhostCscGraph, GhostCsrGraph, GhostDag};
pub use token::{GhostToken, SharedGhostToken};

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
