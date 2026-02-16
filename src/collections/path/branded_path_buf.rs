use crate::GhostCell;
use crate::token::traits::GhostBorrow;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::fmt;

use super::BrandedOsString;

/// A branded PathBuf that can only be accessed using a token of the same brand.
///
/// This provides a safe, mutable path type that integrates with the GhostCell branding system.
#[repr(transparent)]
pub struct BrandedPathBuf<'brand> {
    pub(crate) inner: GhostCell<'brand, PathBuf>,
}

impl<'brand> BrandedPathBuf<'brand> {
    /// Creates an empty `BrandedPathBuf`.
    pub fn new() -> Self {
        Self {
            inner: GhostCell::new(PathBuf::new()),
        }
    }

    /// Creates a `BrandedPathBuf` with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: GhostCell::new(PathBuf::with_capacity(capacity)),
        }
    }

    /// Extends `self` with `path`.
    pub fn push<P: AsRef<Path>>(&mut self, path: P) {
        self.inner.get_mut().push(path);
    }

    /// Truncates `self` to `self.parent`.
    pub fn pop(&mut self) -> bool {
        self.inner.get_mut().pop()
    }

    /// Updates this `BrandedPathBuf` to have the file name `file_name`.
    pub fn set_file_name<S: AsRef<OsStr>>(&mut self, file_name: S) {
        self.inner.get_mut().set_file_name(file_name);
    }

    /// Updates this `BrandedPathBuf` to have the extension `extension`.
    pub fn set_extension<S: AsRef<OsStr>>(&mut self, extension: S) -> bool {
        self.inner.get_mut().set_extension(extension)
    }

    /// Clears the path.
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

    /// Shrinks the capacity of the path as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.inner.get_mut().shrink_to_fit();
    }

    /// Returns the contents as a `Path`.
    pub fn as_path<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> &'a Path {
        self.inner.borrow(token).as_path()
    }

    /// Consumes the `BrandedPathBuf` and returns a `BrandedOsString`.
    pub fn into_os_string(self) -> BrandedOsString<'brand> {
        BrandedOsString {
            inner: GhostCell::new(self.inner.into_inner().into_os_string()),
        }
    }

    /// Clones the BrandedPathBuf using the token.
    pub fn clone_with_token<'a>(&'a self, token: &'a impl GhostBorrow<'brand>) -> Self {
        Self {
            inner: GhostCell::new(self.inner.borrow(token).clone()),
        }
    }
}

impl<'brand> From<PathBuf> for BrandedPathBuf<'brand> {
    fn from(p: PathBuf) -> Self {
        Self {
            inner: GhostCell::new(p),
        }
    }
}

impl<'brand> From<&Path> for BrandedPathBuf<'brand> {
    fn from(p: &Path) -> Self {
        Self {
            inner: GhostCell::new(p.to_path_buf()),
        }
    }
}

impl<'brand> Default for BrandedPathBuf<'brand> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand> fmt::Debug for BrandedPathBuf<'brand> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrandedPathBuf")
         .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_branded_path_buf() {
        GhostToken::new(|mut token| {
            let mut p = BrandedPathBuf::new();
            p.push("foo");
            p.push("bar");

            assert_eq!(p.as_path(&token), Path::new("foo/bar"));

            p.set_extension("txt");
            assert_eq!(p.as_path(&token), Path::new("foo/bar.txt"));

            let popped = p.pop();
            assert!(popped);
            assert_eq!(p.as_path(&token), Path::new("foo"));

            let cloned = p.clone_with_token(&token);
            assert_eq!(cloned.as_path(&token), Path::new("foo"));

            p.clear();
            assert_eq!(p.as_path(&token), Path::new(""));
            assert_eq!(cloned.as_path(&token), Path::new("foo"));
        });
    }

    #[test]
    fn test_into_os_string() {
        GhostToken::new(|token| {
            let mut p = BrandedPathBuf::new();
            p.push("foo");

            let s = p.into_os_string();
            assert_eq!(s.as_os_str(&token), OsStr::new("foo"));
        });
    }
}
