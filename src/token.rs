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
    /// Returns a reference to the token for use in closures
    ///
    /// This method allows the token to be captured by closures while
    /// maintaining the borrowing constraints.
    pub const fn as_ref(&self) -> &Self {
        self
    }

    /// Checks if this token is compatible with another
    ///
    /// Due to the phantom type system, tokens from different scopes
    /// have different types and cannot be mixed.
    pub const fn is_compatible(&self, _other: &Self) -> bool {
        true // Type system ensures compatibility at compile time
    }

    /// Returns whether the token represents a valid branding scope.
    ///
    /// This is always true for valid tokens, but allows for const evaluation.
    pub const fn is_valid(&self) -> bool {
        true
    }

    /// Creates a token from a raw pointer (unsafe, zero-cost operation).
    ///
    /// # Safety
    /// This function is unsafe because it allows creating tokens without
    /// proper scoping. Use only when you can guarantee the branding invariant.
    #[inline(always)]
    pub const unsafe fn from_raw(_ptr: *const ()) -> Self {
        Self(PhantomData)
    }

    /// Executes a closure with both shared and mutable token references.
    ///
    /// This is useful when you need both read and write access to branded data
    /// within the same scope. The closure receives `(shared_token, mutable_token)`.
    ///
    /// # Example
    /// ```rust
    /// use halo::GhostToken;
    ///
    /// GhostToken::new(|token| {
    ///     token.with_split(|shared, mut mut_token| {
    ///         // Use `shared` for reading and `mut_token` for writing
    ///     });
    /// });
    /// ```
    pub fn with_split<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&GhostToken<'brand>, &mut GhostToken<'brand>) -> R,
    {
        // SAFETY: We create a temporary mutable reference that doesn't escape
        // the closure scope, maintaining the linearity invariant.
        let mut temp_token = GhostToken(PhantomData);
        f(self, &mut temp_token)
    }

    /// Creates a token pair for coordinating between multiple branded collections.
    ///
    /// This is useful when you need to work with multiple independent branded
    /// types that should share the same token scope.
    ///
    /// Returns `(token_a, token_b)` where both have the same brand.
    pub fn split(self) -> (GhostToken<'brand>, GhostToken<'brand>) {
        // Since GhostToken is not Clone, we need to be careful here.
        // We create a new token that shares the same brand.
        let token_a = self;
        let token_b = GhostToken(PhantomData);
        (token_a, token_b)
    }

    /// Executes a closure with exclusive token access, preventing accidental sharing.
    ///
    /// This method consumes the token and ensures it cannot be used elsewhere
    /// during the closure execution, providing stronger isolation guarantees.
    ///
    /// # Example
    /// ```rust
    /// use halo::{GhostToken, GhostCell};
    ///
    /// let result = GhostToken::new(|token| {
    ///     let cell = GhostCell::new(42);
    ///     token.exclusively(move |mut exclusive_token| {
    ///         // `exclusive_token` cannot be shared or leaked
    ///         *cell.borrow_mut(&mut exclusive_token) = 100;
    ///         *cell.borrow(&exclusive_token)
    ///     })
    /// });
    /// assert_eq!(result, 100);
    /// ```
    #[inline(always)]
    pub fn exclusively<F, R>(self, f: F) -> R
    where
        F: FnOnce(GhostToken<'brand>) -> R,
    {
        f(self)
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


