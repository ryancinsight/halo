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

/// Global singleton tokens for static lifetime branding.
pub mod global;
/// Hierarchical tokens allowing splitting and restricted views.
pub mod hierarchy;
/// Invariant lifetime definitions for branding.
pub mod invariant;
/// Macros for convenient token generation.
pub mod macros;
/// Shared tokens for reference-counted access.
pub mod shared;
/// Traits defining token capabilities (GhostBorrow/GhostBorrowMut).
pub mod traits;

pub use global::{static_token, with_static_token, with_static_token_mut, StaticBrand};
pub use hierarchy::{HierarchicalGhostToken, ImmutableChild};
pub use invariant::InvariantLifetime;
pub use shared::SharedGhostToken;
pub use traits::{GhostBorrow, GhostBorrowMut};

/// A zero-sized token that controls access to GhostCells
///
/// The token uses a phantom type parameter to create branded types,
/// ensuring type-level separation between different token scopes.
#[derive(Debug)]
pub struct GhostToken<'brand>(InvariantLifetime<'brand>);

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
        f(GhostToken(InvariantLifetime::default()))
    }

    // NOTE: we intentionally keep the public surface small. If you need a
    // `&mut GhostToken<'brand>` for iterator pipelines, just take a mutable
    // borrow of the token inside the `new` closure.

    /// Creates a GhostToken from a raw invariant lifetime.
    ///
    /// # Safety
    ///
    /// This is an internal API. The caller must ensure that the lifetime `'brand`
    /// is used correctly to enforce linearity and uniqueness where required.
    #[inline(always)]
    pub(crate) const fn from_invariant(invariant: InvariantLifetime<'brand>) -> Self {
        GhostToken(invariant)
    }
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

    /// Creates a new branded scope nested within the current one.
    ///
    /// This is functionally equivalent to `GhostToken::new`, but allows
    /// method-chaining style and clarifies intent when creating sub-scopes
    /// for temporary views or nested data structures.
    ///
    /// # Example
    ///
    /// ```
    /// use halo::GhostToken;
    ///
    /// GhostToken::new(|mut token| {
    ///     // Do some work with `token`
    ///
    ///     // Create a temporary sub-scope
    ///     token.with_scoped(|mut sub_token| {
    ///         // Work with `sub_token` is isolated
    ///     });
    /// });
    /// ```
    #[inline(always)]
    pub fn with_scoped<F, R>(&self, f: F) -> R
    where
        F: for<'sub> FnOnce(GhostToken<'sub>) -> R,
    {
        Self::new(f)
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
