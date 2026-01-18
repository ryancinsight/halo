//! `BrandedString` — a mutable string with token-gated access.
//!
//! This provides a safe, mutable string type that integrates with the
//! GhostCell branding system.
//!
//! # Design
//!
//! Unlike `GhostCell<String>`, which locks the entire string container (including length and capacity),
//! `BrandedString` owns the container metadata (`len`, `capacity`) but brands the *content* (bytes).
//!
//! This allows:
//! - **Structural inspection without a token**: `len()`, `capacity()`, `is_empty()`
//! - **Structural mutation without a token**: `push_str()`, `clear()`, `reserve()`
//! - **Content access requires a token**: `as_str()`
//!
//! This follows the same pattern as `BrandedVec`.

use crate::{GhostCell, GhostToken};
use std::fmt;
use std::mem;

/// A branded string compatible with GhostCell.
///
/// This struct manages a buffer of branded bytes, enforcing UTF-8 validity
/// while allowing structural operations without a token.
pub struct BrandedString<'brand> {
    /// The underlying storage.
    /// We use `Vec<GhostCell<'brand, u8>>` which matches the layout of `Vec<u8>`.
    vec: Vec<GhostCell<'brand, u8>>,
}

impl<'brand> BrandedString<'brand> {
    /// Creates a new empty branded string.
    #[inline]
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
        }
    }

    /// Creates a new branded string with the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: Vec::with_capacity(capacity),
        }
    }

    /// Creates a branded string from an existing String.
    #[inline]
    pub fn from_string(s: String) -> Self {
        // SAFETY: `GhostCell<u8>` has the same layout as `u8`.
        // `Vec<GhostCell<u8>>` has the same layout as `Vec<u8>`.
        // We take ownership of the String's vector.
        let bytes = s.into_bytes();
        let vec = unsafe { mem::transmute::<Vec<u8>, Vec<GhostCell<'brand, u8>>>(bytes) };
        Self { vec }
    }

    /// Returns a shared reference to the string slice.
    ///
    /// Requires a token to prove permission to read the branded bytes.
    #[inline]
    pub fn as_str<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a str {
        // SAFETY:
        // 1. `Vec<GhostCell<u8>>` layout == `Vec<u8>`.
        // 2. We maintain UTF-8 invariant in all mutation methods.
        // 3. Token proves access permission.
        unsafe {
            let slice = std::slice::from_raw_parts(
                self.vec.as_ptr() as *const u8,
                self.vec.len()
            );
            std::str::from_utf8_unchecked(slice)
        }
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
            // Cast &mut Vec<GhostCell<u8>> to &mut Vec<u8>
            let vec_ptr = &mut self.vec as *mut Vec<GhostCell<'brand, u8>>;
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

        // We need to check UTF-8 boundary.
        // To do this, we need to inspect the byte at new_len.
        // We don't have a token, but we are `&mut self`.
        // The token protects against *aliased* access.
        // Since we have `&mut self`, we have exclusive access to the container.
        // But the *values* inside `GhostCell` are logically protected.
        // However, `GhostCell` protects against data races and shared mutation.
        // If we are `&mut self`, we can drop elements (remove them) without a token.
        // Can we read them?
        // `truncate` logic in `String` checks `is_char_boundary`.
        // This requires reading the byte.
        // Reading the byte requires a token?
        // Technically yes, `GhostCell::borrow` needs a token.
        // But wait, `GhostCell` owns the value.
        // If we own the `GhostCell` (via `&mut Vec`), we can get `&mut T` via `get_mut` on `GhostCell`.
        // `GhostCell::get_mut` requires `&mut self`.
        // So we CAN read the byte if we treat it as mutable access to the cell itself?
        // `GhostCell` does NOT have `get_mut`?
        // `GhostCell` wraps `UnsafeCell`. `UnsafeCell` has `get_mut`.
        // Let's check `GhostCell` API.

        // Actually, we can use `as_str` logic but we don't have a token.
        // BUT, we are implementing the string itself.
        // If we implement `is_char_boundary` manually:
        // A char boundary is where byte is not a continuation byte (0b10xxxxxx).
        // (byte & 0xC0) != 0x80.

        // We can access the raw byte via `UnsafeCell` / pointer cast since we have `&mut self`.
        // Safety: We have exclusive access to `self`, so no other thread/alias can be reading via token.
        // Reading the byte to check boundary is safe.

        if self.is_char_boundary_internal(new_len) {
            self.vec.truncate(new_len);
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
        // We don't have a token, but we are effectively the "kernel" of this type.
        // Is it sound to read without token?
        // If someone else has `&token`, they could have `&str` (shared ref).
        // If we have `&self` (shared ref), we co-exist.
        // They might be reading. We are reading. Safe.
        // Wait, `truncate` takes `&mut self`.
        // So no one else has `&self`.
        // So no one has `&str`.
        // So it's safe to read.
        unsafe {
            let ptr = self.vec.as_ptr() as *const u8;
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
}
