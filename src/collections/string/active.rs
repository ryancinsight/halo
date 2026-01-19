//! `ActiveString` — a BrandedString bundled with its GhostToken.
//!
//! This wrapper significantly reduces "token redundancy" when performing multiple operations
//! in a single scope. By holding the token exclusively, it can expose a standard `String`-like
//! API without requiring the token as an argument for every call.

use crate::GhostToken;
use super::BrandedString;
use std::fmt;
use std::str::Chars;

/// A wrapper around a mutable reference to a `BrandedString` and a mutable reference to a `GhostToken`.
///
/// This type acts as an "active handle" to the string, allowing mutation and access without
/// repeatedly passing the token.
pub struct ActiveString<'a, 'brand> {
    string: &'a mut BrandedString<'brand>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand> ActiveString<'a, 'brand> {
    /// Creates a new active string handle.
    pub fn new(string: &'a mut BrandedString<'brand>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { string, token }
    }

    /// Returns the length of the string.
    #[inline]
    pub fn len(&self) -> usize {
        self.string.len()
    }

    /// Returns `true` if the string is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    /// Returns the capacity of the string.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.string.capacity()
    }

    /// Reserves capacity for at least `additional` more bytes.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.string.reserve(additional);
    }

    /// Clears the string.
    #[inline]
    pub fn clear(&mut self) {
        self.string.clear();
    }

    /// Appends a string slice.
    #[inline]
    pub fn push_str(&mut self, string: &str) {
        self.string.push_str(string);
    }

    /// Appends a character.
    #[inline]
    pub fn push(&mut self, ch: char) {
        self.string.push(ch);
    }

    /// Truncates the string to `new_len`.
    #[inline]
    pub fn truncate(&mut self, new_len: usize) {
        self.string.truncate(new_len);
    }

    /// Returns a shared reference to the string slice.
    ///
    /// This is the key benefit of ActiveString: getting `&str` without passing a token explicitly.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.string.as_str(self.token)
    }

    /// Returns a byte slice of the string's contents.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.string.as_bytes(self.token)
    }

    /// Returns an iterator over the chars of a string slice.
    #[inline]
    pub fn chars(&self) -> Chars<'_> {
        self.as_str().chars()
    }

    /// Returns an iterator over the bytes of a string slice.
    #[inline]
    pub fn bytes(&self) -> std::str::Bytes<'_> {
        self.as_str().bytes()
    }
}

impl<'a, 'brand> fmt::Display for ActiveString<'a, 'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl<'a, 'brand> fmt::Debug for ActiveString<'a, 'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

/// Extension trait to easily create ActiveString from BrandedString.
pub trait ActivateString<'brand> {
    /// Activates the string with the given token, returning a handle that bundles them.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveString<'a, 'brand>;
}

impl<'brand> ActivateString<'brand> for BrandedString<'brand> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveString<'a, 'brand> {
        ActiveString::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_string_workflow() {
        GhostToken::new(|mut token| {
            let mut s = BrandedString::new();
            s.push_str("Hello");

            // Activate scope
            {
                let mut active = s.activate(&mut token);
                active.push_str(", ");
                active.push('W');
                active.push_str("orld");

                assert_eq!(active.len(), 12);
                assert_eq!(active.as_str(), "Hello, World");

                // Test Display and Debug
                assert_eq!(format!("{}", active), "Hello, World");
                assert_eq!(format!("{:?}", active), "\"Hello, World\"");
            }

            // Token released
            assert_eq!(s.as_str(&token), "Hello, World");
        });
    }

    #[test]
    fn test_active_string_iterators() {
         GhostToken::new(|mut token| {
            let mut s = BrandedString::from("ABC");
            let active = s.activate(&mut token);

            let chars: Vec<char> = active.chars().collect();
            assert_eq!(chars, vec!['A', 'B', 'C']);

            let bytes: Vec<u8> = active.bytes().collect();
            assert_eq!(bytes, vec![65, 66, 67]);
        });
    }
}
