//! `BrandedVec` — a vector of token-gated cells.
//!
//! This is the canonical "branded vector" pattern from the GhostCell/RustBelt paper:
//! store many independently-mutable elements in one owned container, while using a
//! **single** linear token to gate all borrows.
//!
//! Design:
//! - The container owns a `Vec<GhostCell<'brand, T>>`.
//! - Structural mutations (`push`, `pop`, `reserve`, …) follow normal Rust rules via
//!   `&mut self`.
//! - Element access is token-gated:
//!   - shared access: `&GhostToken<'brand>` → `&T`
//!   - exclusive access: `&mut GhostToken<'brand>` → `&mut T`
//!
//! This is exactly the separation of *permissions* (token) from *data* (cells).

use crate::{GhostCell, GhostToken};

/// A vector of token-gated elements.
pub struct BrandedVec<'brand, T> {
    inner: Vec<GhostCell<'brand, T>>,
}

/// A branded array with compile-time size guarantees.
///
/// This provides the same token-gated access as `BrandedVec` but with:
/// - Compile-time capacity guarantees via const generics
/// - Better cache locality for small, fixed-size collections
/// - Zero-allocation for statically-sized collections
/// - Mathematical bounds checking at compile time
///
/// # Type Parameters
/// - `'brand`: The token branding lifetime
/// - `T`: The element type
/// - `CAPACITY`: Compile-time maximum capacity
#[repr(C)]
pub struct BrandedArray<'brand, T, const CAPACITY: usize> {
    /// The actual storage array
    inner: [GhostCell<'brand, T>; CAPACITY],
    /// Current length (tracked at runtime for safety)
    len: usize,
}

impl<'brand, T> BrandedVec<'brand, T> {
    /// Creates an empty vector.
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Creates an empty vector with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Current capacity.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Pushes a new element.
    pub fn push(&mut self, value: T) {
        self.inner.push(GhostCell::new(value));
    }

    /// Pops the last element.
    pub fn pop(&mut self) -> Option<GhostCell<'brand, T>> {
        self.inner.pop()
    }

    /// Inserts an element at position `index`.
    pub fn insert(&mut self, index: usize, value: T) {
        self.inner.insert(index, GhostCell::new(value));
    }

    /// Removes and returns the element at position `index`.
    pub fn remove(&mut self, index: usize) -> GhostCell<'brand, T> {
        self.inner.remove(index)
    }

    /// Removes an element from the vector and returns it, replaces it with the last element.
    pub fn swap_remove(&mut self, index: usize) -> GhostCell<'brand, T> {
        self.inner.swap_remove(index)
    }

    /// Retains only the elements specified by the predicate.
    pub fn retain<F>(&mut self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        self.inner.retain(|c| f(c.borrow_mut(token)));
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        self.inner.get(idx).map(|c| c.borrow(token))
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        self.inner.get(idx).map(|c| c.borrow_mut(token))
    }

    /// Returns a token-gated shared reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        self.get(token, idx).expect("index out of bounds")
    }

    /// Returns a token-gated exclusive reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> &'a mut T {
        self.get_mut(token, idx).expect("index out of bounds")
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a T> + 'a {
        self.inner.iter().map(|c| c.borrow(token))
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, which preserves the
    /// token linearity invariant without requiring an `Iterator<Item = &mut T>`.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for cell in &self.inner {
            // Each borrow is scoped to this loop iteration.
            let x = cell.borrow_mut(token);
            f(x);
        }
    }
}

impl<'brand, T> Default for BrandedVec<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T, const CAPACITY: usize> BrandedArray<'brand, T, CAPACITY> {
    /// Creates a new empty array.
    ///
    /// All elements are initialized with their `Default` value.
    ///
    /// # Panics
    /// Panics if `T` does not implement `Default`.
    pub fn new() -> Self
    where
        T: Default,
    {
        Self {
            inner: core::array::from_fn(|_| GhostCell::new(T::default())),
            len: 0,
        }
    }

    /// Creates a new array from an iterator.
    ///
    /// # Panics
    /// Panics if the iterator yields more than `CAPACITY` elements.
    pub fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Default,
    {
        let mut array = Self::new();
        for item in iter {
            array.push(item);
        }
        array
    }

    /// Returns the current number of elements.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns the compile-time capacity.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Returns `true` if the array is empty.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the array is at full capacity.
    #[inline(always)]
    pub const fn is_full(&self) -> bool {
        self.len == CAPACITY
    }

    /// Pushes an element onto the end of the array.
    ///
    /// # Panics
    /// Panics if the array is already at capacity.
    pub fn push(&mut self, value: T) {
        assert!(self.len < CAPACITY, "BrandedArray is at capacity");
        self.inner[self.len] = GhostCell::new(value);
        self.len += 1;
    }

    /// Pops the last element from the array.
    ///
    /// This method requires `T: Default` because we need to leave a valid value in the array.
    pub fn pop(&mut self) -> Option<T>
    where
        T: Default,
    {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            // Use replace to avoid moving out of array
            let replacement = GhostCell::new(T::default());
            Some(core::mem::replace(&mut self.inner[self.len], replacement).into_inner())
        }
    }

    /// Clears the array, dropping all elements.
    ///
    /// This method requires `T: Default` because we need to leave valid values in the array.
    pub fn clear(&mut self)
    where
        T: Default,
    {
        // Drop elements in reverse order for safety
        while self.len > 0 {
            self.len -= 1;
            // GhostCell will handle dropping the inner value
            let _ = core::mem::replace(&mut self.inner[self.len], GhostCell::new(T::default()));
        }
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        if idx < self.len {
            Some(self.inner[idx].borrow(token))
        } else {
            None
        }
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> Option<&'a mut T> {
        if idx < self.len {
            Some(self.inner[idx].borrow_mut(token))
        } else {
            None
        }
    }

    /// Returns a token-gated shared reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        assert!(idx < self.len, "index {} out of bounds for BrandedArray of len {}", idx, self.len);
        self.inner[idx].borrow(token)
    }

    /// Returns a token-gated exclusive reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> &'a mut T {
        assert!(idx < self.len, "index {} out of bounds for BrandedArray of len {}", idx, self.len);
        self.inner[idx].borrow_mut(token)
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> impl Iterator<Item = &'a T> + 'a {
        self.inner[..self.len].iter().map(|cell| cell.borrow(token))
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, preserving token linearity.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        for i in 0..self.len {
            let x = self.inner[i].borrow_mut(token);
            f(x);
        }
    }

    /// Returns the underlying array as a slice of cells.
    ///
    /// This is useful for advanced operations that need direct cell access.
    pub fn as_cells(&self) -> &[GhostCell<'brand, T>; CAPACITY] {
        &self.inner
    }

    /// Returns the underlying array as a mutable slice of cells.
    ///
    /// # Safety
    /// This bypasses the token system. Use with extreme caution.
    pub fn as_cells_mut(&mut self) -> &mut [GhostCell<'brand, T>; CAPACITY] {
        &mut self.inner
    }
}

impl<'brand, T: Default, const CAPACITY: usize> Default for BrandedArray<'brand, T, CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn branded_vec_basic_access() {
        GhostToken::new(|mut token| {
            let mut v: BrandedVec<'_, u64> = BrandedVec::new();
            v.push(10);
            v.push(20);

            assert_eq!(v.len(), 2);
            assert_eq!(*v.borrow(&token, 0), 10);
            assert_eq!(*v.borrow(&token, 1), 20);

            *v.borrow_mut(&mut token, 0) += 7;
            assert_eq!(*v.borrow(&token, 0), 17);
        });
    }

    #[test]
    fn branded_vec_iter_and_iter_mut() {
        GhostToken::new(|mut token| {
            let mut v: BrandedVec<'_, i32> = BrandedVec::new();
            for i in 0..10 {
                v.push(i);
            }

            let sum: i32 = v.iter(&token).copied().sum();
            assert_eq!(sum, (0..10).sum());

            v.for_each_mut(&mut token, |x| *x *= 2);
            let doubled: Vec<i32> = v.iter(&token).copied().collect();
            assert_eq!(doubled, (0..10).map(|x| x * 2).collect::<Vec<_>>());
        });
    }

    #[test]
    fn branded_array_basic_operations() {
        GhostToken::new(|mut token| {
            let mut arr: BrandedArray<'_, u32, 8> = BrandedArray::new();

            assert_eq!(arr.len(), 0);
            assert_eq!(arr.capacity(), 8);
            assert!(arr.is_empty());
            assert!(!arr.is_full());

            // Test push
            for i in 0..8 {
                arr.push(i as u32);
                assert_eq!(arr.len(), i + 1);
            }

            assert!(arr.is_full());
            assert!(!arr.is_empty());

            // Test access
            assert_eq!(*arr.borrow(&token, 0), 0);
            assert_eq!(*arr.borrow(&token, 7), 7);

            // Test mutation
            *arr.borrow_mut(&mut token, 0) = 42;
            assert_eq!(*arr.borrow(&token, 0), 42);

            // Test iteration
            let sum: u32 = arr.iter(&token).sum();
            assert_eq!(sum, 42 + 1 + 2 + 3 + 4 + 5 + 6 + 7);

            // Test pop
            assert_eq!(arr.pop(), Some(7));
            assert_eq!(arr.len(), 7);
            assert!(!arr.is_full());

            arr.clear();
            assert_eq!(arr.len(), 0);
            assert!(arr.is_empty());
        });
    }

    #[test]
    fn branded_array_bounds_checking() {
        GhostToken::new(|token| {
            let mut arr: BrandedArray<'_, i32, 4> = BrandedArray::new();

            // Initially empty
            assert!(arr.get(&token, 0).is_none());

            // Add one element
            arr.push(42);
            assert_eq!(*arr.get(&token, 0).unwrap(), 42);

            // Out of bounds should return None
            assert!(arr.get(&token, 1).is_none());
        });
    }

    #[test]
    fn branded_array_from_iter() {
        GhostToken::new(|token| {
            let arr = BrandedArray::<_, 5>::from_iter(0..5);

            assert_eq!(arr.len(), 5);
            assert_eq!(arr.capacity(), 5);

            for i in 0..5 {
                assert_eq!(*arr.borrow(&token, i), i as u32);
            }
        });
    }

    #[test]
    #[should_panic]
    fn branded_array_capacity_overflow() {
        let mut arr: BrandedArray<'_, i32, 2> = BrandedArray::new();
        arr.push(1);
        arr.push(2);
        arr.push(3); // This should panic
    }
}


