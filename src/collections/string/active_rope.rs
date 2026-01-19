//! `ActiveRope` — a BrandedRope bundled with its GhostToken.
//!
//! This wrapper reduces "token redundancy" and provides a cleaner API for Rope operations.

use crate::GhostToken;
use super::rope::{BrandedRope, CharsIter, BytesIter};
use std::fmt;

/// A wrapper around a mutable reference to a `BrandedRope` and a mutable reference to a `GhostToken`.
pub struct ActiveRope<'a, 'brand> {
    rope: &'a mut BrandedRope<'brand>,
    token: &'a mut GhostToken<'brand>,
}

impl<'a, 'brand> ActiveRope<'a, 'brand> {
    /// Creates a new active rope handle.
    pub fn new(rope: &'a mut BrandedRope<'brand>, token: &'a mut GhostToken<'brand>) -> Self {
        Self { rope, token }
    }

    /// Returns the length of the rope.
    #[inline]
    pub fn len(&self) -> usize {
        self.rope.len()
    }

    /// Returns `true` if the rope is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rope.is_empty()
    }

    /// Clears the rope.
    #[inline]
    pub fn clear(&mut self) {
        self.rope.clear();
    }

    /// Appends a string slice.
    #[inline]
    pub fn append(&mut self, s: &str) {
        self.rope.append(self.token, s);
    }

    /// Inserts a string at the given byte index.
    #[inline]
    pub fn insert(&mut self, idx: usize, s: &str) {
        self.rope.insert(self.token, idx, s);
    }

    /// Gets the byte at index.
    #[inline]
    pub fn get_byte(&self, index: usize) -> Option<u8> {
        self.rope.get_byte(self.token, index)
    }

    /// Returns an iterator over the bytes of the rope.
    pub fn bytes<'b>(&'b self) -> BytesIter<'b, 'brand> {
        self.rope.bytes(self.token)
    }

    /// Returns an iterator over the chars of the rope.
    pub fn chars<'b>(&'b self) -> CharsIter<'b, 'brand> {
        self.rope.chars(self.token)
    }
}

impl<'a, 'brand> fmt::Display for ActiveRope<'a, 'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for char in self.chars() {
            write!(f, "{}", char)?;
        }
        Ok(())
    }
}

impl<'a, 'brand> fmt::Debug for ActiveRope<'a, 'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"")?;
        for char in self.chars() {
             write!(f, "{}", char.escape_debug())?;
        }
        write!(f, "\"")
    }
}

/// Extension trait to easily create ActiveRope from BrandedRope.
pub trait ActivateRope<'brand> {
    /// Activates the rope with the given token.
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRope<'a, 'brand>;
}

impl<'brand> ActivateRope<'brand> for BrandedRope<'brand> {
    fn activate<'a>(&'a mut self, token: &'a mut GhostToken<'brand>) -> ActiveRope<'a, 'brand> {
        ActiveRope::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_rope_workflow() {
        GhostToken::new(|mut token| {
            let mut rope = BrandedRope::new();

            {
                let mut active = rope.activate(&mut token);
                active.append("Hello");
                active.append(" World");

                assert_eq!(active.len(), 11);
                assert_eq!(format!("{}", active), "Hello World");

                active.insert(5, ",");
                assert_eq!(format!("{}", active), "Hello, World");
            }
        });
    }
}
