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
        // We can't easily construct an array of non-Copy/Clone items without MaybeUninit or loop.
        // But ImmutableChild IS Copy? No, we didn't derive Copy.
        // The requirement says "Children can be freely borrowed concurrently."
        // And "No Copy/Clone on mutable-capable tokens."
        // ReadOnly tokens *could* be Copy?
        // If they are Copy, then `split` is just copying.

        // Let's assume they are NOT Copy to maintain linearity discipline if we want to merge them later?
        // Requirement: "Provide merging/joining to recover the parent capability (linear discipline)."
        // This implies linearity even for immutable children if we split a *Mutable* token into immutable ones?
        // But here we are splitting `&self` (immutable borrow).
        // We can't merge back to `self` because we don't own it.

        // However, if we have `&mut self` (exclusive), we can split into immutable children and then merge back?
        // The method signature in req: `fn split_immutable(&self)`.

        // Let's implement `split_into` using array map or similar.
        // Since it's ZST, we can just use `[ImmutableChild::new(); N]` conceptually.

        use core::mem::MaybeUninit;

        // Safe because ZST and we construct them safely from &self
        unsafe {
            let mut arr: [MaybeUninit<ImmutableChild<'_, 'brand>>; N] = MaybeUninit::uninit().assume_init();
            for i in 0..N {
                arr[i] = MaybeUninit::new(HierarchicalGhostToken::new());
            }
            // Transmute to initialized array
            (&arr as *const _ as *const [ImmutableChild<'_, 'brand>; N]).read()
        }
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
