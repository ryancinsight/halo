use core::marker::PhantomData;

/// A marker type that is invariant in its lifetime parameter `'id`.
///
/// This is used to ensure that brands cannot be subtyped (shrunken) by the compiler,
/// preventing different data structures from being unified under the same brand
/// when they shouldn't be.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvariantLifetime<'id>(PhantomData<fn(&'id ()) -> &'id ()>);

impl<'id> InvariantLifetime<'id> {
    /// Creates a new invariant lifetime marker.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}
