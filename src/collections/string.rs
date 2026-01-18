//! `BrandedString` — a mutable string with token-gated access.
//!
//! This provides a safe, mutable string type that integrates with the
//! GhostCell branding system. It allows sharing the string reference
//! while restricting mutation to the token owner.
//!
//! # Zero-Cost Abstraction
//!
//! `BrandedString` is a thin wrapper around `GhostCell<String>`.
//! It has the same memory layout as `GhostCell<String>` (which is `UnsafeCell<String>`),
//! and thus the same layout as `String`.

use crate::{GhostCell, GhostToken};
use std::fmt;

/// A branded string compatible with GhostCell.
///
/// This is a wrapper around `GhostCell<String>` that provides a convenient
/// `String`-like API.
#[repr(transparent)]
pub struct BrandedString<'brand> {
    inner: GhostCell<'brand, String>,
}

impl<'brand> BrandedString<'brand> {
    /// Creates a new empty branded string.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: GhostCell::new(String::new()),
        }
    }

    /// Creates a new branded string with the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: GhostCell::new(String::with_capacity(capacity)),
        }
    }

    /// Creates a branded string from an existing String.
    #[inline]
    pub fn from_string(s: String) -> Self {
        Self {
            inner: GhostCell::new(s),
        }
    }

    /// Returns a shared reference to the string slice.
    #[inline]
    pub fn as_str<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a str {
        self.inner.borrow(token).as_str()
    }

    /// Appends a string slice.
    #[inline]
    pub fn push_str(&self, token: &mut GhostToken<'brand>, string: &str) {
        self.inner.borrow_mut(token).push_str(string);
    }

    /// Appends a character.
    #[inline]
    pub fn push(&self, token: &mut GhostToken<'brand>, ch: char) {
        self.inner.borrow_mut(token).push(ch);
    }

    /// Returns the length of the string.
    #[inline]
    pub fn len(&self, token: &GhostToken<'brand>) -> usize {
        self.inner.borrow(token).len()
    }

    /// Returns true if the string is empty.
    #[inline]
    pub fn is_empty(&self, token: &GhostToken<'brand>) -> bool {
        self.inner.borrow(token).is_empty()
    }

    /// Returns the capacity of the string.
    #[inline]
    pub fn capacity(&self, token: &GhostToken<'brand>) -> usize {
        self.inner.borrow(token).capacity()
    }

    /// Reserves capacity for at least `additional` more bytes.
    #[inline]
    pub fn reserve(&self, token: &mut GhostToken<'brand>, additional: usize) {
        self.inner.borrow_mut(token).reserve(additional);
    }

    /// Clears the string.
    #[inline]
    pub fn clear(&self, token: &mut GhostToken<'brand>) {
        self.inner.borrow_mut(token).clear();
    }

    /// Truncates the string to `new_len`.
    #[inline]
    pub fn truncate(&self, token: &mut GhostToken<'brand>, new_len: usize) {
        self.inner.borrow_mut(token).truncate(new_len);
    }

    /// Returns a mutable reference to the underlying String.
    /// Use with caution.
    #[inline]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>) -> &'a mut String {
        self.inner.borrow_mut(token)
    }
}

impl<'brand> Default for BrandedString<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> From<String> for BrandedString<'brand> {
    fn from(s: String) -> Self {
        Self::from_string(s)
    }
}

impl<'brand> From<&str> for BrandedString<'brand> {
    fn from(s: &str) -> Self {
        Self::from_string(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branded_string_basic() {
        GhostToken::new(|mut token| {
            let s = BrandedString::new();
            s.push_str(&mut token, "hello");
            s.push(&mut token, ' ');
            s.push_str(&mut token, "world");

            assert_eq!(s.as_str(&token), "hello world");
            assert_eq!(s.len(&token), 11);
            assert!(!s.is_empty(&token));

            s.clear(&mut token);
            assert!(s.is_empty(&token));
        });
    }

    #[test]
    fn test_branded_string_capacity() {
        GhostToken::new(|mut token| {
            let s = BrandedString::with_capacity(10);
            assert!(s.capacity(&token) >= 10);

            s.reserve(&mut token, 20);
            assert!(s.capacity(&token) >= 20);
        });
    }

    #[test]
    fn test_branded_string_from() {
        GhostToken::new(|token| {
            let s1 = BrandedString::from("test");
            assert_eq!(s1.as_str(&token), "test");

            let s2 = BrandedString::from_string("test2".to_string());
            assert_eq!(s2.as_str(&token), "test2");
        });
    }
}
