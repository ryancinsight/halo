use core::marker::PhantomData;
use crate::token::GhostToken;
use crate::token::traits::{GhostBorrow, GhostBorrowMut};

/// Marker trait for token permissions.
pub trait Permission {}

/// Permission representing read-only access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOnly;

/// Permission representing full read-write access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullAccess;

impl Permission for ReadOnly {}
impl Permission for FullAccess {}

/// A hierarchical token derived from a parent token.
///
/// It carries a lifetime `'parent` ensuring it cannot outlive the parent token,
/// a `'brand` matching the data it protects, and a `Perm` indicating capabilities.
pub struct HierarchicalGhostToken<'parent, 'brand, Perm: Permission> {
    _marker: PhantomData<&'parent GhostToken<'brand>>,
    _perm: PhantomData<Perm>,
}

impl<'parent, 'brand, Perm: Permission> HierarchicalGhostToken<'parent, 'brand, Perm> {
    /// Creates a new hierarchical token.
    ///
    /// # Safety
    /// Must ensure that the parent token is legitimately borrowed/owned and
    /// permissions are a subset.
    pub(crate) unsafe fn new() -> Self {
        Self {
            _marker: PhantomData,
            _perm: PhantomData,
        }
    }
}

// Access traits
impl<'parent, 'brand, Perm: Permission> GhostBorrow<'brand> for HierarchicalGhostToken<'parent, 'brand, Perm> {}

impl<'parent, 'brand> GhostBorrowMut<'brand> for HierarchicalGhostToken<'parent, 'brand, FullAccess> {}

// Type alias for convenience
/// A hierarchical token with read-only permissions.
pub type ImmutableChild<'parent, 'brand> = HierarchicalGhostToken<'parent, 'brand, ReadOnly>;

impl<'brand> GhostToken<'brand> {
    /// Splits the token view into two immutable children.
    ///
    /// Since `&self` represents shared access, we can derive multiple immutable
    /// tokens that are valid as long as `self` is borrowed.
    pub fn split_immutable<'a>(&'a self) -> (ImmutableChild<'a, 'brand>, ImmutableChild<'a, 'brand>) {
        unsafe {
            (
                HierarchicalGhostToken::new(),
                HierarchicalGhostToken::new(),
            )
        }
    }

    /// Splits the token view into N immutable children.
    pub fn split_into<const N: usize>(&self) -> [ImmutableChild<'_, 'brand>; N] {
        // Since ImmutableChild is Copy, we can just create one and copy it.
        [unsafe { HierarchicalGhostToken::new() }; N]
    }
}

impl<'brand> GhostToken<'brand> {
    /// Creates a hierarchical token with full access permissions.
    ///
    /// This borrows the parent token exclusively, preventing any other access
    /// until the child token is dropped.
    pub fn borrow_mut<'a>(&'a mut self) -> HierarchicalGhostToken<'a, 'brand, FullAccess> {
        unsafe { HierarchicalGhostToken::new() }
    }
    
    /// Creates a hierarchical token with read-only permissions.
    pub fn borrow<'a>(&'a self) -> HierarchicalGhostToken<'a, 'brand, ReadOnly> {
        unsafe { HierarchicalGhostToken::new() }
    }
}

impl<'parent, 'brand> HierarchicalGhostToken<'parent, 'brand, FullAccess> {
    /// Downgrades a full access token to a read-only token.
    pub fn downgrade(self) -> HierarchicalGhostToken<'parent, 'brand, ReadOnly> {
        unsafe { HierarchicalGhostToken::new() }
    }
}

// Allow Copy/Clone for ReadOnly tokens?
// "No Copy/Clone on mutable-capable tokens."
// Implies ReadOnly *can* be Copy.
impl<'parent, 'brand> Clone for HierarchicalGhostToken<'parent, 'brand, ReadOnly> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
            _perm: PhantomData,
        }
    }
}

impl<'parent, 'brand> Copy for HierarchicalGhostToken<'parent, 'brand, ReadOnly> {}
