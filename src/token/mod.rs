//! GhostToken - The permission controller for GhostCell
//!
//! The GhostToken is a zero-sized type that controls access to a collection
//! of GhostCells. It uses phantom types and rank-2 polymorphism to ensure
//! that only cells created within the same token's scope can be accessed.
//!
//! ## Core invariant (linearity)
//!
//! `GhostToken<'brand>` is intentionally **not** `Copy`/`Clone`.
//! This makes it a *linear* capability: any safe API that can produce `&mut T`
//! requires `&mut GhostToken<'brand>`, and Rust guarantees you cannot have two
//! live mutable borrows of the same token simultaneously.

use core::marker::PhantomData;

pub mod shared;
pub use shared::SharedGhostToken;

/// A zero-sized token that controls access to GhostCells
///
/// The token uses a phantom type parameter to create branded types,
/// ensuring type-level separation between different token scopes.
#[derive(Debug)]
pub struct GhostToken<'brand>(PhantomData<&'brand mut ()>);

impl<'brand> GhostToken<'brand> {
    /// Creates a new token and executes a closure with it
    ///
    /// This is the primary way to create and use GhostTokens. The closure
    /// receives a mutable reference to the token, allowing it to be used
    /// to create and manipulate GhostCells.
    ///
    /// # Example
    ///
    /// ```rust
    /// use halo::{GhostToken, GhostCell};
    ///
    /// let result = GhostToken::new(|mut token| {
    ///     let cell = GhostCell::new(42);
    ///     *cell.borrow_mut(&mut token) = 100;
    ///     *cell.borrow(&token)
    /// });
    /// assert_eq!(result, 100);
    /// ```
    pub fn new<F, R>(f: F) -> R
    where
        F: for<'new_brand> FnOnce(GhostToken<'new_brand>) -> R,
    {
        f(GhostToken(PhantomData))
    }

    // NOTE: we intentionally keep the public surface small. If you need a
    // `&mut GhostToken<'brand>` for iterator pipelines, just take a mutable
    // borrow of the token inside the `new` closure.
}

impl<'brand> GhostToken<'brand> {
    /// Returns a reference to the token (useful for capturing in closures).
    #[inline(always)]
    pub const fn as_ref(&self) -> &Self {
        self
    }

    /// Returns whether the token represents a valid branding scope.
    ///
    /// This is always true for valid tokens, but allows for const evaluation.
    #[inline(always)]
    pub const fn is_valid(&self) -> bool {
        true
    }
}

// NOTE:
// `GhostToken` is intentionally NOT `Copy`/`Clone`.
//
// This token is a linear capability: duplicating it would allow creating multiple
// simultaneous `&mut GhostToken<'brand>` values (by taking `&mut` of two copies),
// which would break the core exclusivity invariant needed to safely expose `&mut T`
// from `&Ghost*Cell<T>`.

// Concurrency notes:
// - `GhostToken<'brand>` contains no data and exists only as a compile-time capability.
// - Making it `Sync` is sound: sharing `&GhostToken<'brand>` across threads only enables
//   immutable, token-gated reads (`&T`), which are already constrained by `T: Sync` via
//   the usual `Sync` bounds on the cells themselves.
// - Exclusive mutation still requires `&mut GhostToken<'brand>`, which Rust borrowing
//   prevents from coexisting with any shared references to the same token.
unsafe impl<'brand> Sync for GhostToken<'brand> {}
