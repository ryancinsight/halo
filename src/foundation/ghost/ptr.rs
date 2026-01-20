use crate::GhostToken;
use crate::token::InvariantLifetime;
use core::fmt;
use core::ptr::NonNull;

/// A non-null pointer that is branded with a unique lifetime `'id`.
///
/// This pointer functions like `NonNull<T>`, but access to the underlying data
/// is gated by a `GhostToken<'id>`.
///
/// # Invariance
/// The lifetime `'id` is invariant, ensuring that brands cannot be coerced.
/// The type `T` is covariant, matching standard `NonNull<T>` behavior.
#[repr(transparent)]
pub struct BrandedNonNull<'id, T> {
    ptr: NonNull<T>,
    _brand: InvariantLifetime<'id>,
}

impl<'id, T> BrandedNonNull<'id, T> {
    /// Creates a new `BrandedNonNull`.
    ///
    /// # Safety
    /// `ptr` must be non-null.
    #[inline(always)]
    pub unsafe fn new_unchecked(ptr: *mut T) -> Self {
        Self {
            ptr: NonNull::new_unchecked(ptr),
            _brand: InvariantLifetime::default(),
        }
    }

    /// Creates a new `BrandedNonNull` if `ptr` is non-null.
    #[inline(always)]
    pub fn new(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| Self {
            ptr,
            _brand: InvariantLifetime::default(),
        })
    }

    /// Creates a `BrandedNonNull` from a standard `NonNull`.
    #[inline(always)]
    pub fn from_non_null(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _brand: InvariantLifetime::default(),
        }
    }

    /// Returns the internal `NonNull` pointer.
    #[inline(always)]
    pub fn as_non_null(self) -> NonNull<T> {
        self.ptr
    }

    /// Returns the raw pointer.
    #[inline(always)]
    pub fn as_ptr(self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Access the value immutably using the token.
    ///
    /// # Safety
    /// The caller must ensure that the pointer is valid (not dangling) and
    /// properly aligned. The token guarantees permission to access the brand.
    #[inline(always)]
    pub unsafe fn borrow<'a>(&self, _token: &'a GhostToken<'id>) -> &'a T {
        self.ptr.as_ref()
    }

    /// Access the value mutably using the token.
    ///
    /// # Safety
    /// The caller must ensure that the pointer is valid (not dangling) and
    /// properly aligned. The token guarantees exclusive permission to access the brand.
    #[inline(always)]
    pub unsafe fn borrow_mut<'a>(&self, _token: &'a mut GhostToken<'id>) -> &'a mut T {
        &mut *self.ptr.as_ptr()
    }
}

impl<'id, T> Copy for BrandedNonNull<'id, T> {}
impl<'id, T> Clone for BrandedNonNull<'id, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'id, T> fmt::Debug for BrandedNonNull<'id, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BrandedNonNull")
         .field(&self.ptr)
         .finish()
    }
}

impl<'id, T> Eq for BrandedNonNull<'id, T> {}
impl<'id, T> PartialEq for BrandedNonNull<'id, T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}
