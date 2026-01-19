//! `BrandedString` — a mutable string with token-gated access.
//!
//! This provides a safe, mutable string type that integrates with the
//! GhostCell branding system.
//!
//! # Design
//!
//! This implementation wraps [`BrandedVec<u8>`](crate::collections::BrandedVec) to provide
//! string-specific operations while harnessing the existing token-gated safety mechanisms.
//!
//! It allows:
//! - **Structural inspection without a token**: `len()`, `capacity()`, `is_empty()`
//! - **Structural mutation without a token**: `push_str()`, `clear()`, `reserve()`
//! - **Content access requires a token**: `as_str()`
//!
//! # Implementation Note
//!
//! To achieve high performance for structural mutations (like `push_str`), `BrandedString`
//! accesses the internal storage of `BrandedVec` directly (via `pub(crate)` visibility)
//! to perform bulk operations without per-byte token overhead, while trusting `BrandedVec`'s
//! branding guarantees.

use crate::collections::BrandedVec;
use crate::{GhostCell, GhostToken};
use std::mem;

/// A branded string compatible with GhostCell.
///
/// This struct manages a buffer of branded bytes, enforcing UTF-8 validity
/// while allowing structural operations without a token.
#[repr(transparent)]
pub struct BrandedString<'brand> {
    /// The underlying branded vector of bytes.
    vec: BrandedVec<'brand, u8>,
}

impl<'brand> BrandedString<'brand> {
    /// Creates a new empty branded string.
    #[inline]
    pub fn new() -> Self {
        Self {
            vec: BrandedVec::new(),
        }
    }

    /// Creates a new branded string with the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: BrandedVec::with_capacity(capacity),
        }
    }

    /// Creates a branded string from an existing String.
    #[inline]
    pub fn from_string(s: String) -> Self {
        // SAFETY: `GhostCell<u8>` has the same layout as `u8`.
        // `Vec<GhostCell<u8>>` has the same layout as `Vec<u8>`.
        // We take ownership of the String's vector and wrap it in BrandedVec.
        let bytes = s.into_bytes();
        let inner_vec = unsafe { mem::transmute::<Vec<u8>, Vec<GhostCell<'brand, u8>>>(bytes) };
        Self {
            vec: BrandedVec { inner: inner_vec },
        }
    }

    /// Returns a shared reference to the string slice.
    ///
    /// Requires a token to prove permission to read the branded bytes.
    #[inline]
    pub fn as_str<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a str {
        // Leverage BrandedVec's safe token-gated slice access
        let slice = self.vec.as_slice(token);

        // SAFETY: We maintain UTF-8 invariant in all mutation methods.
        unsafe { std::str::from_utf8_unchecked(slice) }
    }

    /// Returns a byte slice of this string's contents.
    ///
    /// Requires a token to prove permission to read the branded bytes.
    #[inline]
    pub fn as_bytes<'a>(&'a self, token: &'a GhostToken<'brand>) -> &'a [u8] {
        self.vec.as_slice(token)
    }

    /// Appends a string slice.
    ///
    /// Does NOT require a token because we are owners of the structure and
    /// we are appending new, valid values.
    #[inline]
    pub fn push_str(&mut self, string: &str) {
        // SAFETY:
        // 1. `Vec<GhostCell<u8>>` layout == `Vec<u8>`.
        // 2. Appending valid UTF-8 bytes to a valid UTF-8 string maintains validity.
        unsafe {
            // Access the inner vector directly for performance
            // Cast &mut Vec<GhostCell<u8>> to &mut Vec<u8>
            let vec_ptr = &mut self.vec.inner as *mut Vec<GhostCell<'brand, u8>>;
            let vec_u8_ptr = vec_ptr as *mut Vec<u8>;
            let vec_u8 = &mut *vec_u8_ptr;
            vec_u8.extend_from_slice(string.as_bytes());
        }
    }

    /// Appends a character.
    #[inline]
    pub fn push(&mut self, ch: char) {
        // Encode char to bytes
        let mut buf = [0; 4];
        let s = ch.encode_utf8(&mut buf);
        self.push_str(s);
    }

    /// Returns the length of the string.
    ///
    /// Does NOT require a token.
    #[inline]
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns true if the string is empty.
    ///
    /// Does NOT require a token.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Returns the capacity of the string.
    ///
    /// Does NOT require a token.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    /// Reserves capacity for at least `additional` more bytes.
    ///
    /// Does NOT require a token.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.vec.reserve(additional);
    }

    /// Clears the string.
    ///
    /// Does NOT require a token.
    #[inline]
    pub fn clear(&mut self) {
        self.vec.clear();
    }

    /// Truncates the string to `new_len`.
    ///
    /// Does NOT require a token, but must respect UTF-8 boundaries.
    ///
    /// # Panics
    /// Panics if `new_len` does not lie on a `char` boundary.
    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        if new_len >= self.len() {
            return;
        }

        if self.is_char_boundary_internal(new_len) {
            // BrandedVec doesn't expose truncate, but we can access inner
            self.vec.inner.truncate(new_len);
        } else {
            panic!("new_len does not lie on a char boundary");
        }
    }

    fn is_char_boundary_internal(&self, index: usize) -> bool {
        if index == 0 {
            return true;
        }
        if index == self.len() {
            return true;
        }
        if index > self.len() {
            return false;
        }

        // Read byte at index
        // SAFETY: index is in bounds. We have `&self`.
        // We are reading a byte to check its bit pattern.
        // We safely read from the inner vector using raw pointer access via UnsafeCell/GhostCell.
        unsafe {
            let ptr = self.vec.inner.as_ptr() as *const u8;
            let byte = *ptr.add(index);
            // Check if it's NOT a continuation byte (10xxxxxx)
            (byte as i8) >= -0x40
        }
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
        // No token needed for creation and structural mutation!
        let mut s = BrandedString::new();
        s.push_str("hello");
        s.push(' ');
        s.push_str("world");

        assert_eq!(s.len(), 11);
        assert!(!s.is_empty());

        // Token needed for reading content
        GhostToken::new(|token| {
            assert_eq!(s.as_str(&token), "hello world");
        });

        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn test_branded_string_capacity() {
        let mut s = BrandedString::with_capacity(10);
        assert!(s.capacity() >= 10);

        s.reserve(20);
        assert!(s.capacity() >= 20);
    }

    #[test]
    fn test_branded_string_from() {
        let s1 = BrandedString::from("test");
        let s2 = BrandedString::from_string("test2".to_string());

        GhostToken::new(|token| {
            assert_eq!(s1.as_str(&token), "test");
            assert_eq!(s2.as_str(&token), "test2");
        });
    }

    #[test]
    fn test_branded_string_truncate() {
        let mut s = BrandedString::from("hello world");
        s.truncate(5);

        GhostToken::new(|token| {
            assert_eq!(s.as_str(&token), "hello");
        });
    }

    #[test]
    #[should_panic]
    fn test_branded_string_truncate_panic() {
        let mut s = BrandedString::from("héllo"); // 'é' is 2 bytes
                                                  // 'h' is index 0. 'é' starts at 1. next char at 3.
        s.truncate(2); // Mid-char boundary of 'é'
    }

    #[test]
    fn test_branded_string_as_bytes() {
        let mut s = BrandedString::from("abc");
        GhostToken::new(|token| {
            assert_eq!(s.as_bytes(&token), b"abc");
        });
    }
}
