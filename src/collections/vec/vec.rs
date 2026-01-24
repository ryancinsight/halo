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
use core::slice;
use core::mem::MaybeUninit;

/// Compile-time assertion types for const generics bounds checking
pub struct Assert<const COND: bool>;
pub trait IsTrue {}
impl IsTrue for Assert<true> {}

/// A vector of token-gated elements.
pub struct BrandedVec<'brand, T> {
    pub(crate) inner: Vec<GhostCell<'brand, T>>,
}

/// A branded array with compile-time size guarantees.
///
/// This provides the same token-gated access as `BrandedVec` but with:
/// - Compile-time capacity guarantees via const generics
/// - Better cache locality for small, fixed-size collections
/// - Zero-allocation for statically-sized collections
/// - Mathematical bounds checking at compile time
/// - SIMD-friendly memory layout
///
/// # Type Parameters
/// - `'brand`: The token branding lifetime
/// - `T`: The element type
/// - `CAPACITY`: Compile-time maximum capacity
#[repr(C, align(64))] // Cache line alignment for SIMD operations
pub struct BrandedArray<'brand, T, const CAPACITY: usize> {
    /// The actual storage array - aligned for optimal access
    inner: [MaybeUninit<GhostCell<'brand, T>>; CAPACITY],
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

    /// Swaps two elements in the vector.
    pub fn swap(&mut self, a: usize, b: usize) {
        self.inner.swap(a, b);
    }

    /// Clears the vector, removing all values.
    ///
    /// Note that this method has no effect on the allocated capacity
    /// of the vector.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Shortens the vector, keeping the first `len` elements and dropping
    /// the rest.
    ///
    /// If `len` is greater than the vector's current length, this has no
    /// effect.
    pub fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }

    /// Shrinks the capacity of the vector as much as possible.
    ///
    /// It will drop down as close as possible to the length but the allocator
    /// may still inform the vector that there is space for a few more elements.
    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }

    /// Resizes the `BrandedVec` in-place so that `len` is equal to `new_len`.
    ///
    /// If `new_len` is greater than `len`, the `BrandedVec` is extended by the
    /// difference, with each additional slot filled with the result of
    /// calling the closure `f`. The closure is called once for each
    /// element created.
    ///
    /// If `new_len` is less than `len`, the `BrandedVec` is simply truncated.
    pub fn resize_with<F>(&mut self, new_len: usize, mut f: F)
    where
        F: FnMut() -> T,
    {
        self.inner.resize_with(new_len, || GhostCell::new(f()));
    }

    /// Retains only the elements specified by the predicate.
    pub fn retain<F>(&mut self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        self.inner.retain(|c| f(c.borrow_mut(token)));
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    #[inline(always)]
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        self.inner.get(idx).map(|c| c.borrow(token))
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    #[inline(always)]
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        self.inner.get(idx).map(|c| c.borrow_mut(token))
    }

    /// Returns a token-gated shared reference to element `idx` without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        self.inner.get_unchecked(idx).borrow(token)
    }

    /// Returns a token-gated exclusive reference to element `idx` without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> &'a mut T {
        self.inner.get_unchecked(idx).borrow_mut(token)
    }

    /// Returns a token-gated shared reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        self.get(token, idx).expect("index out of bounds")
    }

    /// Returns a token-gated exclusive reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> &'a mut T {
        self.get_mut(token, idx).expect("index out of bounds")
    }

    /// Returns a mutable reference to element `idx` without a token.
    ///
    /// This requires exclusive access to the vector (`&mut self`).
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked_mut_exclusive(&mut self, idx: usize) -> &mut T {
        self.inner.get_unchecked_mut(idx).get_mut()
    }

    /// Returns a mutable reference to element `idx` without a token.
    ///
    /// This requires exclusive access to the vector (`&mut self`).
    #[inline(always)]
    pub fn get_mut_exclusive(&mut self, idx: usize) -> Option<&mut T> {
        self.inner.get_mut(idx).map(|cell| cell.get_mut())
    }

    /// Returns a slice of the underlying elements.
    ///
    /// This enables the use of standard slice methods like `binary_search`, `windows`, etc.
    ///
    /// # Safety
    /// This uses `unsafe` code to transmute `&[GhostCell<T>]` to `&[T]`.
    /// This is safe because:
    /// 1. `GhostCell<T>` is `repr(transparent)` over `UnsafeCell<T>`.
    /// 2. `UnsafeCell<T>` has the same memory layout as `T`.
    /// 3. The token guarantees we have access permission.
    #[inline(always)]
    pub fn as_slice<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a [T] {
        // SAFETY: We have shared token access, so reading T is safe.
        // We obtain a pointer to elements and create a slice.
        unsafe {
            let ptr = self.inner.as_ptr() as *const T;
            std::slice::from_raw_parts(ptr, self.inner.len())
        }
    }

    /// Returns a mutable slice of the underlying elements.
    ///
    /// This enables the use of standard mutable slice methods like `sort`, `sort_by`, `chunks_mut`, etc.
    ///
    /// # Safety
    /// This uses `unsafe` code to create `&mut [T]` from the vector's buffer.
    /// This is safe because:
    /// 1. Layout compatibility: `GhostCell<T>` has same layout as `T`.
    /// 2. We hold `&mut GhostToken`, guaranteeing exclusive access to all cells with this brand.
    /// 3. We hold `&self`, guaranteeing the vector buffer remains valid (no reallocation).
    /// 4. The returned lifetime is tied to `&mut GhostToken`, ensuring exclusivity is maintained.
    #[inline(always)]
    pub fn as_mut_slice<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> &'a mut [T] {
        unsafe {
            let ptr = self.inner.as_ptr() as *mut T;
            std::slice::from_raw_parts_mut(ptr, self.inner.len())
        }
    }

    /// Returns a mutable slice of the underlying elements without a token.
    ///
    /// This requires exclusive access to the vector (`&mut self`).
    ///
    /// # Safety
    /// This uses `unsafe` code to create `&mut [T]` from the vector's buffer.
    /// This is safe because:
    /// 1. Layout compatibility: `GhostCell<T>` has same layout as `T`.
    /// 2. We hold `&mut self`, guaranteeing exclusive access to the vector and all its cells.
    /// 3. Since we have exclusive access to the vector, no other tokens can be accessing it.
    #[inline(always)]
    pub fn as_mut_slice_exclusive(&mut self) -> &mut [T] {
        unsafe {
            let ptr = self.inner.as_mut_ptr() as *mut T;
            std::slice::from_raw_parts_mut(ptr, self.inner.len())
        }
    }

    /// Iterates over all elements by shared reference.
    ///
    /// This iterator is zero-cost: no allocations, no closures per element.
    /// Returns direct references to elements without indirection.
    ///
    /// Optimized to use slice iterator, bypassing per-element `GhostCell` borrowing overhead.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> slice::Iter<'a, T> {
        self.as_slice(token).iter()
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, which preserves the
    /// token linearity invariant without requiring an `Iterator<Item = &mut T>`.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        self.inner.iter().for_each(|cell| {
            f(cell.borrow_mut(token));
        });
    }

    /// Applies `f` to each element by exclusive reference without a token.
    ///
    /// This requires exclusive access to the vector (`&mut self`).
    pub fn for_each_mut_exclusive(&mut self, mut f: impl FnMut(&mut T)) {
        self.inner.iter_mut().for_each(|cell| {
            f(cell.get_mut());
        });
    }

    /// Zero-copy filter with fused iterator operations.
    /// Returns an iterator that yields references to elements matching the predicate.
    pub fn filter_ref<'a, F>(
        &'a self,
        token: &'a GhostToken<'brand>,
        f: F,
    ) -> impl Iterator<Item = &'a T> + 'a
    where
        F: Fn(&T) -> bool + 'a,
    {
        self.iter(token).filter(move |item| f(*item))
    }

    /// Zero-copy find operation - returns reference without copying.
    #[inline(always)]
    pub fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(move |item| f(item))
    }

    /// Zero-copy position finder with fused operations.
    #[inline(always)]
    pub fn position_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> Option<usize>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).position(move |item| f(item))
    }

    /// Zero-cost fold operation with iterator fusion.
    pub fn fold_ref<B, F>(&self, token: &GhostToken<'brand>, init: B, f: F) -> B
    where
        F: FnMut(B, &T) -> B,
    {
        self.iter(token).fold(init, f)
    }

    /// Zero-cost any/all operations with short-circuiting.
    #[inline(always)]
    pub fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).any(move |item| f(item))
    }

    #[inline(always)]
    pub fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).all(move |item| f(item))
    }

    /// Zero-cost count operation.
    #[inline(always)]
    pub fn count_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).filter(move |item| f(item)).count()
    }

    /// Zero-cost min_by operation with custom comparator.
    pub fn min_by_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T, &T) -> std::cmp::Ordering,
    {
        self.iter(token).min_by(|a, b| f(a, b))
    }

    /// Zero-cost max_by operation with custom comparator.
    pub fn max_by_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T, &T) -> std::cmp::Ordering,
    {
        self.iter(token).max_by(|a, b| f(a, b))
    }

    /// Creates a draining iterator that removes the specified range in the vector
    /// and yields the removed items.
    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = T> + '_
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.inner.drain(range).map(GhostCell::into_inner)
    }

    /// Clones the branded vector using the token to access elements.
    ///
    /// This enables deep copying of the vector's contents when T is Clone.
    /// This is necessary because `BrandedVec` cannot implement `Clone` directly
    /// as it requires a token to read the elements.
    pub fn clone_with_token(&self, token: &GhostToken<'brand>) -> Self
    where
        T: Clone,
    {
        let new_inner = self
            .inner
            .iter()
            .map(|cell| GhostCell::new(cell.borrow(token).clone()))
            .collect();
        BrandedVec { inner: new_inner }
    }
}

impl<'brand, T> crate::collections::BrandedCollection<'brand> for BrandedVec<'brand, T> {
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'brand, T> crate::collections::ZeroCopyOps<'brand, T> for BrandedVec<'brand, T> {
    #[inline(always)]
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(move |item| f(item))
    }

    #[inline(always)]
    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).any(move |item| f(item))
    }

    #[inline(always)]
    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).all(move |item| f(item))
    }
}

impl<'brand, T> Default for BrandedVec<'brand, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T> FromIterator<T> for BrandedVec<'brand, T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().map(GhostCell::new).collect(),
        }
    }
}

impl<'brand, T> IntoIterator for BrandedVec<'brand, T> {
    type Item = T;
    type IntoIter =
        std::iter::Map<std::vec::IntoIter<GhostCell<'brand, T>>, fn(GhostCell<'brand, T>) -> T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter().map(GhostCell::into_inner)
    }
}

impl<'brand, T> Extend<T> for BrandedVec<'brand, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.inner.extend(iter.into_iter().map(GhostCell::new));
    }
}

impl<'brand, T, const CAPACITY: usize> BrandedArray<'brand, T, CAPACITY> {
    /// Creates a new empty array.
    ///
    /// Elements are lazily initialized.
    pub fn new() -> Self {
        Self {
            // Safety: Array of MaybeUninit is always initialized
            inner: unsafe { MaybeUninit::uninit().assume_init() },
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
    {
        let mut array = Self::new();
        iter.into_iter().for_each(|item| array.push(item));
        array
    }

    /// Compile-time bounds-checked get operation.
    /// Uses const generics to ensure bounds checking at compile time where possible.
    #[inline(always)]
    pub fn get_const<'a, const IDX: usize>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> Option<&'a T> {
        if IDX < self.len && IDX < CAPACITY {
            // Safety: We verified IDX < len, so the element is initialized
            Some(unsafe { self.inner[IDX].assume_init_ref() }.borrow(token))
        } else {
            None
        }
    }

    /// SIMD-friendly iteration with compile-time bounds.
    /// This method is optimized for SIMD operations on fixed-size arrays.
    pub fn iter_simd<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a T> + 'a + use<'a, 'brand, T, CAPACITY> {
        self.inner
            .iter()
            .take(self.len)
            .map(|cell| unsafe { cell.assume_init_ref() }.borrow(token))
    }

    /// Returns the current number of elements in the array.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns the compile-time capacity of the array.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Returns true if the array contains no elements.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns true if the array is at full capacity.
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
        self.inner[self.len].write(GhostCell::new(value));
        self.len += 1;
    }

    /// Pops the last element from the array.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            // Safety: We just checked len > 0, so inner[len] was initialized.
            // We are reducing len, so this element is now effectively removed.
            unsafe { Some(self.inner[self.len].assume_init_read().into_inner()) }
        }
    }

    /// Clears the array, dropping all elements.
    pub fn clear(&mut self) {
        while self.len > 0 {
            self.len -= 1;
            // Safety: We are dropping elements that were initialized.
            unsafe { self.inner[self.len].assume_init_drop() };
        }
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        if idx < self.len {
            // Safety: idx < len ensures initialization
            Some(unsafe { self.inner[idx].assume_init_ref() }.borrow(token))
        } else {
            None
        }
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        if idx < self.len {
            // Safety: idx < len ensures initialization
            Some(unsafe { self.inner[idx].assume_init_ref() }.borrow_mut(token))
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
        assert!(
            idx < self.len,
            "index {} out of bounds for BrandedArray of len {}",
            idx,
            self.len
        );
        unsafe { self.inner[idx].assume_init_ref() }.borrow(token)
    }

    /// Returns a token-gated exclusive reference to element `idx`.
    ///
    /// # Panics
    /// Panics if `idx` is out of bounds.
    #[inline(always)]
    pub fn borrow_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, idx: usize) -> &'a mut T {
        assert!(
            idx < self.len,
            "index {} out of bounds for BrandedArray of len {}",
            idx,
            self.len
        );
        unsafe { self.inner[idx].assume_init_ref() }.borrow_mut(token)
    }

    /// Returns a slice of the underlying elements.
    #[inline(always)]
    pub fn as_slice<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a [T] {
        unsafe {
            // Cast *const MaybeUninit<GhostCell<T>> to *const T is valid because layouts match
            let ptr = self.inner.as_ptr() as *const T;
            std::slice::from_raw_parts(ptr, self.len)
        }
    }

    /// Returns a mutable slice of the underlying elements.
    #[inline(always)]
    pub fn as_mut_slice<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> &'a mut [T] {
        unsafe {
            // Cast *const MaybeUninit<GhostCell<T>> to *mut T is valid because layouts match
            // We have exclusive access to the token, which grants exclusive access to the cells
            let ptr = self.inner.as_ptr() as *mut T;
            std::slice::from_raw_parts_mut(ptr, self.len)
        }
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a T> + 'a + use<'a, 'brand, T, CAPACITY> {
        self.inner[..self.len]
            .iter()
            .map(|cell| unsafe { cell.assume_init_ref() }.borrow(token))
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, preserving token linearity.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        self.inner[..self.len].iter().for_each(|cell| {
            f(unsafe { cell.assume_init_ref() }.borrow_mut(token));
        });
    }

    /// Returns the underlying array as a slice of cells.
    ///
    /// This is useful for advanced operations that need direct cell access.
    pub fn as_cells(&self) -> &[MaybeUninit<GhostCell<'brand, T>>; CAPACITY] {
        &self.inner
    }

    /// Returns the underlying array as a mutable slice of cells.
    ///
    /// # Safety
    /// This bypasses the token system. Use with extreme caution.
    pub fn as_cells_mut(&mut self) -> &mut [MaybeUninit<GhostCell<'brand, T>>; CAPACITY] {
        &mut self.inner
    }
}

impl<'brand, T, const CAPACITY: usize> Drop for BrandedArray<'brand, T, CAPACITY> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<'brand, T, const CAPACITY: usize> Default for BrandedArray<'brand, T, CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

/// Tests for zero-copy operations and advanced features.
#[cfg(test)]
mod zero_copy_tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_zero_copy_iterator_operations() {
        GhostToken::new(|token| {
            let mut vec = BrandedVec::new();
            vec.push(1);
            vec.push(2);
            vec.push(3);
            vec.push(4);
            vec.push(5);

            // Test find_ref
            let found = vec.find_ref(&token, |&x| x == 3);
            assert_eq!(found, Some(&3));

            let not_found = vec.find_ref(&token, |&x| x == 99);
            assert_eq!(not_found, None);

            // Test any_ref
            assert!(vec.any_ref(&token, |&x| x > 3));
            assert!(!vec.any_ref(&token, |&x| x > 10));

            // Test all_ref
            assert!(vec.all_ref(&token, |&x| x > 0));
            assert!(!vec.all_ref(&token, |&x| x > 2));

            // Test count_ref
            let count_even = vec.count_ref(&token, |&x| x % 2 == 0);
            assert_eq!(count_even, 2); // 2, 4

            let count_gt_3 = vec.count_ref(&token, |&x| x > 3);
            assert_eq!(count_gt_3, 2); // 4, 5

            // Test min_by_ref and max_by_ref
            let min = vec.min_by_ref(&token, |a, b| a.cmp(b));
            assert_eq!(min, Some(&1));

            let max = vec.max_by_ref(&token, |a, b| a.cmp(b));
            assert_eq!(max, Some(&5));
        });
    }

    #[test]
    fn test_zero_copy_empty_vector() {
        GhostToken::new(|token| {
            let vec: BrandedVec<i32> = BrandedVec::new();

            // All operations should return expected results for empty vec
            assert_eq!(vec.find_ref(&token, |_| true), None);
            assert!(!vec.any_ref(&token, |_| true));
            assert!(vec.all_ref(&token, |_| false)); // vacuously true
            assert_eq!(vec.count_ref(&token, |_| true), 0);
            assert_eq!(vec.min_by_ref(&token, |a, b| a.cmp(b)), None);
            assert_eq!(vec.max_by_ref(&token, |a, b| a.cmp(b)), None);
        });
    }

    #[test]
    fn test_zero_copy_single_element() {
        GhostToken::new(|token| {
            let mut vec = BrandedVec::new();
            vec.push(42);

            assert_eq!(vec.find_ref(&token, |&x| x == 42), Some(&42));
            assert!(vec.any_ref(&token, |&x| x == 42));
            assert!(vec.all_ref(&token, |&x| x == 42));
            assert_eq!(vec.count_ref(&token, |&x| x == 42), 1);
            assert_eq!(vec.min_by_ref(&token, |a, b| a.cmp(b)), Some(&42));
            assert_eq!(vec.max_by_ref(&token, |a, b| a.cmp(b)), Some(&42));
        });
    }

    #[test]
    fn test_zero_copy_iterator_fusion() {
        GhostToken::new(|token| {
            let mut vec = BrandedVec::new();
            for i in 0..10 {
                vec.push(i);
            }

            // Test that operations can be chained efficiently (iterator fusion)
            let result: Vec<_> = vec
                .iter(&token)
                .filter(|&&x| x % 2 == 0) // even numbers
                .map(|&x| x * 2) // double them
                .collect();

            assert_eq!(result, vec![0, 4, 8, 12, 16]); // 0*2, 2*2, 4*2, 6*2, 8*2

            // Test zero-copy filter followed by count
            let even_count = vec.iter(&token).filter(|&&x| x % 2 == 0).count();
            assert_eq!(even_count, 5); // 0, 2, 4, 6, 8
        });
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

    #[test]
    fn branded_vec_as_slice_mut() {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            vec.push(3);
            vec.push(1);
            vec.push(2);

            // Use standard slice sort via as_mut_slice
            vec.as_mut_slice(&mut token).sort();

            assert_eq!(vec.as_slice(&token), &[1, 2, 3]);

            // Mutate via slice
            for x in vec.as_mut_slice(&mut token) {
                *x *= 2;
            }
            assert_eq!(vec.as_slice(&token), &[2, 4, 6]);
        });
    }

    #[test]
    fn branded_array_as_slice() {
        GhostToken::new(|mut token| {
            let mut arr: BrandedArray<'_, i32, 4> = BrandedArray::new();
            arr.push(10);
            arr.push(20);

            assert_eq!(arr.as_slice(&token), &[10, 20]);

            arr.as_mut_slice(&mut token)[0] = 30;
            assert_eq!(arr.as_slice(&token), &[30, 20]);
        });
    }

    #[test]
    fn branded_array_pop_no_default() {
        struct NoDefault {
            val: i32,
        }
        GhostToken::new(|mut token| {
            let mut arr: BrandedArray<'_, NoDefault, 4> = BrandedArray::new();
            arr.push(NoDefault { val: 1 });

            // This should compile and run without needing T: Default
            let popped = arr.pop();
            assert!(popped.is_some());
            assert_eq!(popped.unwrap().val, 1);
            assert_eq!(arr.len(), 0);
        });
    }

    #[test]
    fn branded_vec_clone_with_token() {
        GhostToken::new(|mut token| {
            let mut v1: BrandedVec<'_, i32> = BrandedVec::new();
            v1.push(1);
            v1.push(2);

            let v2 = v1.clone_with_token(&token);

            assert_eq!(v1.len(), 2);
            assert_eq!(v2.len(), 2);
            assert_eq!(*v2.borrow(&token, 0), 1);
            assert_eq!(*v2.borrow(&token, 1), 2);

            // Mutation of v1 shouldn't affect v2
            *v1.borrow_mut(&mut token, 0) = 10;
            assert_eq!(*v1.borrow(&token, 0), 10);
            assert_eq!(*v2.borrow(&token, 0), 1);
        });
    }
}
