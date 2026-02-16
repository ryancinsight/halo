use crate::GhostCell;
use crate::token::traits::GhostBorrow;
use std::ffi::{OsStr, OsString};
use std::fmt;

/// A branded OsString that can only be accessed using a token of the same brand.
///
/// This provides a safe, mutable string type for OS-native strings that integrates with the
/// GhostCell branding system.
///
/// Unlike `BrandedString`, which wraps a `BrandedVec<u8>`, `BrandedOsString` wraps a
/// `GhostCell<OsString>` directly because `OsString` internals are platform-dependent
/// and opaque. As a result, operations like `len()` require a token, whereas `BrandedString`
/// can provide them without a token.
#[repr(transparent)]
pub struct BrandedOsString<'brand> {
    pub(crate) inner: GhostCell<'brand, OsString>,
}

impl<'brand> BrandedOsString<'brand> {
    /// Creates a new empty `BrandedOsString`.
    pub fn new() -> Self {
        Self {
            inner: GhostCell::new(OsString::new()),
        }
    }

    /// Creates a new `BrandedOsString` with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: GhostCell::new(OsString::with_capacity(capacity)),
        }
    }

    /// Pushes a string slice onto the end of the string.
    ///
    /// This is a structural mutation that does not require a token because we have
    /// exclusive access to the `BrandedOsString`.
    pub fn push<T: AsRef<OsStr>>(&mut self, s: T) {
        self.inner.get_mut().push(s);
    }

    /// Clears the string.
    pub fn clear(&mut self) {
        self.inner.get_mut().clear();
    }

    /// Reserves capacity for at least `additional` more bytes.
    pub fn reserve(&mut self, additional: usize) {
        self.inner.get_mut().reserve(additional);
    }

    /// Reserves the minimum capacity for at least `additional` more bytes.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.get_mut().reserve_exact(additional);
    }

    /// Shrinks the capacity of the string as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.inner.get_mut().shrink_to_fit();
    }

    /// Returns the length of the string.
    pub fn len<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> usize {
        self.inner.borrow(token).len()
    }

    /// Returns true if the string is empty.
    pub fn is_empty<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> bool {
        self.inner.borrow(token).is_empty()
    }

    /// Returns the capacity.
    pub fn capacity<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> usize {
        self.inner.borrow(token).capacity()
    }

    /// Returns the contents as an `OsStr`.
    pub fn as_os_str<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> &'a OsStr {
        self.inner.borrow(token).as_os_str()
    }

    /// Clones the BrandedOsString using the token.
    pub fn clone_with_token<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> Self {
        Self {
            inner: GhostCell::new(self.inner.borrow(token).clone()),
        }
    }
}

impl<'brand> Default for BrandedOsString<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> From<OsString> for BrandedOsString<'brand> {
    fn from(s: OsString) -> Self {
        Self {
            inner: GhostCell::new(s),
        }
    }
}

impl<'brand> From<&str> for BrandedOsString<'brand> {
    fn from(s: &str) -> Self {
        Self::from(OsString::from(s))
    }
}

impl<'brand> From<String> for BrandedOsString<'brand> {
    fn from(s: String) -> Self {
        Self::from(OsString::from(s))
    }
}

impl<'brand> fmt::Debug for BrandedOsString<'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrandedOsString")
         .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_os_string() {
        GhostToken::new(|mut token| {
            let mut s = BrandedOsString::new();
            s.push("hello");
            s.push(" world");

            assert_eq!(s.as_os_str(&token), OsStr::new("hello world"));
            assert_eq!(s.len(&token), 11);
            assert!(!s.is_empty(&token));

            let cloned = s.clone_with_token(&token);
            assert_eq!(cloned.as_os_str(&token), OsStr::new("hello world"));

            s.clear();
            assert!(s.is_empty(&token));
            assert_eq!(s.len(&token), 0);

            // cloned should be unchanged
            assert_eq!(cloned.as_os_str(&token), OsStr::new("hello world"));
        });
    }
}
