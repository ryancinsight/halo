//! `GhostUnsafeCell` — the minimal raw interior-mutation primitive.
//!
//! This is the foundational storage building block for the rest of the crate.
//! It is intentionally small, simple, and (as much as possible) `core`-only.
//!
//! ## Safety invariant (informal but precise)
//!
//! For a fixed brand `'brand`, all safe methods that can produce `&mut T`
//! require `&mut GhostToken<'brand>`. Since `GhostToken<'brand>` is linear
//! (not `Copy`/`Clone`), Rust's borrow rules ensure there cannot exist two
//! simultaneous mutable borrows of the same token, and therefore safe code
//! cannot create overlapping mutable borrows of the same `Ghost*Cell`.
//!
//! All remaining ways to observe or mutate the value are either:
//! - immutable borrows tied to `&GhostToken<'brand>`, or
//! - raw pointers that are `unsafe` to dereference/mutate, placing the burden
//!   on the caller (standard Rust contract for raw pointers).

use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem,
    ptr,
};

use crate::GhostToken;

/// A token-branded wrapper around `core::cell::UnsafeCell<T>`.
#[repr(transparent)]
#[derive(Debug)]
pub struct GhostUnsafeCell<'brand, T: ?Sized>(PhantomData<&'brand mut ()>, UnsafeCell<T>);

impl<'brand, T> GhostUnsafeCell<'brand, T> {
    /// Creates a new cell containing `value`.
    pub const fn new(value: T) -> Self {
        Self(PhantomData, UnsafeCell::new(value))
    }
}

impl<'brand, T: ?Sized> GhostUnsafeCell<'brand, T> {
    #[inline(always)]
    fn raw(&self) -> &UnsafeCell<T> {
        &self.1
    }

    /// Returns a shared reference to the contained value.
    #[inline(always)]
    pub fn get<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a T {
        // SAFETY: safe code cannot obtain `&mut T` without `&mut GhostToken<'brand>`.
        unsafe { &*self.raw().get() }
    }

    /// Returns an exclusive reference to the contained value.
    #[inline(always)]
    pub fn get_mut<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> &'a mut T {
        // SAFETY: caller proves exclusivity via `&mut GhostToken<'brand>`.
        unsafe { &mut *self.raw().get() }
    }

    /// Returns a raw const pointer to the contained value.
    #[inline(always)]
    pub fn as_ptr(&self, _token: &GhostToken<'brand>) -> *const T {
        self.raw().get().cast_const()
    }

    /// Returns a raw mut pointer to the contained value.
    #[inline(always)]
    pub fn as_mut_ptr(&self, _token: &mut GhostToken<'brand>) -> *mut T {
        self.raw().get()
    }

    /// Returns a raw mut pointer to the contained value **without** requiring a token.
    ///
    /// This is crate-only and exists to implement safe `Drop` for higher-level
    /// primitives that must clean up their internal state without having a token.
    ///
    /// The returned pointer must still be treated as a raw pointer: dereferencing
    /// or writing through it is `unsafe` and must uphold aliasing rules.
    #[inline(always)]
    pub(crate) fn as_mut_ptr_unchecked(&self) -> *mut T {
        self.raw().get()
    }
}

impl<'brand, T> GhostUnsafeCell<'brand, T> {
    /// Replaces the contained value, returning the old one.
    #[inline(always)]
    pub fn replace(&self, value: T, token: &mut GhostToken<'brand>) -> T {
        mem::replace(self.get_mut(token), value)
    }

    /// Consumes the cell and returns the contained value.
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.1.into_inner()
    }

    /// Reads the value out of the cell without moving `self`.
    ///
    /// # Safety
    /// This is equivalent to `ptr::read(self.as_ptr(..))` and therefore creates
    /// a bitwise copy; it is only valid if the caller upholds the usual `read`
    /// contract (no double-drop, etc.).
    #[inline(always)]
    pub unsafe fn read(&self, token: &GhostToken<'brand>) -> T {
        // SAFETY: caller upholds `ptr::read` contract.
        unsafe { ptr::read(self.as_ptr(token)) }
    }
}

// SAFETY: sending the cell by value is fine if `T: Send` because it does not
// implicitly grant access to the interior; access still requires the branded token.
unsafe impl<'brand, T: Send> Send for GhostUnsafeCell<'brand, T> {}

// SAFETY: sharing `&GhostUnsafeCell` between threads is fine if `T: Sync` because
// the only safe shared access yields `&T`, and `&T` is thread-safe iff `T: Sync`.
unsafe impl<'brand, T: Sync> Sync for GhostUnsafeCell<'brand, T> {}


