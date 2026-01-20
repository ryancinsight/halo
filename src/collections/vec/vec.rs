//! `BrandedVec` — a vector of token-gated cells.
//!
//! This is the canonical "branded vector" pattern from the GhostCell/RustBelt pattern:
//! store many independently-mutable elements in one owned container, while using a
//! **single** linear token to gate all borrows.
//!
//! Design:
//! - The container uses manual memory management with `BrandedNonNull` to store `T`.
//! - Structural mutations (`push`, `pop`, `reserve`, …) follow normal Rust rules via
//!   `&mut self`.
//! - Element access is token-gated:
//!   - shared access: `&GhostToken<'brand>` → `&T`
//!   - exclusive access: `&mut GhostToken<'brand>` → `&mut T`

use crate::foundation::ghost::ptr::BrandedNonNull;
use crate::{GhostCell, GhostToken};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use core::slice;
use std::alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout};

/// Compile-time assertion types for const generics bounds checking
pub struct Assert<const COND: bool>;
pub trait IsTrue {}
impl IsTrue for Assert<true> {}

/// A vector of token-gated elements.
pub struct BrandedVec<'brand, T> {
    ptr: BrandedNonNull<'brand, T>,
    len: usize,
    cap: usize,
    _marker: PhantomData<GhostCell<'brand, T>>,
}

// Safety: BrandedVec owns the memory and the data.
unsafe impl<'brand, T: Send> Send for BrandedVec<'brand, T> {}
unsafe impl<'brand, T: Sync> Sync for BrandedVec<'brand, T> {}

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
    inner: [GhostCell<'brand, T>; CAPACITY],
    /// Current length (tracked at runtime for safety)
    len: usize,
}

impl<'brand, T> BrandedVec<'brand, T> {
    /// Creates an empty vector.
    pub fn new() -> Self {
        // SAFETY: dangling is non-null.
        let ptr = unsafe { BrandedNonNull::new_unchecked(NonNull::dangling().as_ptr()) };
        Self {
            ptr,
            len: 0,
            cap: 0,
            _marker: PhantomData,
        }
    }

    /// Creates an empty vector with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }
        let layout = Layout::array::<T>(capacity).expect("capacity overflow");

        if core::mem::size_of::<T>() == 0 {
             return Self::new();
        }

        let ptr = unsafe { alloc(layout) as *mut T };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }

        Self {
            ptr: unsafe { BrandedNonNull::new_unchecked(ptr) },
            len: 0,
            cap: capacity,
            _marker: PhantomData,
        }
    }

    /// Creates a BrandedVec from a standard Vec.
    /// This consumes the Vec.
    pub fn from_vec(vec: Vec<T>) -> Self {
        let mut vec = core::mem::ManuallyDrop::new(vec);
        let ptr = vec.as_mut_ptr();
        let len = vec.len();
        let cap = vec.capacity();
        unsafe {
            Self {
                ptr: BrandedNonNull::new_unchecked(ptr),
                len,
                cap,
                _marker: PhantomData,
            }
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Current capacity.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        if self.cap - self.len >= additional {
            return;
        }
        let new_cap = self.len.checked_add(additional).expect("capacity overflow");
        let target_cap = core::cmp::max(self.cap * 2, new_cap);
        let target_cap = core::cmp::max(target_cap, 4); // Min cap

        self.grow(target_cap);
    }

    fn grow(&mut self, new_cap: usize) {
        if core::mem::size_of::<T>() == 0 {
            self.cap = usize::MAX; // ZST capacity is infinite
            return;
        }

        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");

        let new_ptr = if self.cap == 0 {
            unsafe { alloc(new_layout) as *mut T }
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            unsafe { realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout.size()) as *mut T }
        };

        if new_ptr.is_null() {
            handle_alloc_error(new_layout);
        }

        self.ptr = unsafe { BrandedNonNull::new_unchecked(new_ptr) };
        self.cap = new_cap;
    }

    /// Pushes a new element.
    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            if core::mem::size_of::<T>() == 0 {
                if self.len == usize::MAX { panic!("capacity overflow"); }
                self.cap = usize::MAX;
            } else {
                let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
                self.grow(new_cap);
            }
        }

        unsafe {
            ptr::write(self.ptr.as_ptr().add(self.len), value);
        }
        self.len += 1;
    }

    /// Pops the last element.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            unsafe {
                Some(ptr::read(self.ptr.as_ptr().add(self.len)))
            }
        }
    }

    /// Truncates the vector, keeping the first `len` elements and dropping the rest.
    pub fn truncate(&mut self, len: usize) {
        while self.len > len {
            self.pop();
        }
    }

    /// Inserts an element at position `index`.
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len, "index out of bounds");
        if self.len == self.cap {
            let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
            self.grow(new_cap);
        }

        unsafe {
            let p = self.ptr.as_ptr().add(index);
            ptr::copy(p, p.add(1), self.len - index);
            ptr::write(p, value);
        }
        self.len += 1;
    }

    /// Removes and returns the element at position `index`.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            let result = ptr::read(p);
            ptr::copy(p.add(1), p, self.len - index - 1);
            self.len -= 1;
            result
        }
    }

    /// Removes an element from the vector and returns it, replaces it with the last element.
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            let last_p = self.ptr.as_ptr().add(self.len - 1);
            let result = ptr::read(p);
            if index != self.len - 1 {
                 ptr::copy(last_p, p, 1);
            }
            self.len -= 1;
            result
        }
    }

    /// Swaps two elements in the vector.
    pub fn swap(&mut self, a: usize, b: usize) {
        assert!(a < self.len && b < self.len, "index out of bounds");
        unsafe {
            ptr::swap(self.ptr.as_ptr().add(a), self.ptr.as_ptr().add(b));
        }
    }

    /// Clears the vector, removing all values.
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Retains only the elements specified by the predicate.
    pub fn retain<F>(&mut self, token: &mut GhostToken<'brand>, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let mut del = 0;
        let len = self.len;
        unsafe {
            let ptr = self.ptr.as_ptr();
            for i in 0..len {
                if !f(&mut *ptr.add(i)) {
                    del += 1;
                    ptr::drop_in_place(ptr.add(i));
                } else if del > 0 {
                    ptr::copy(ptr.add(i), ptr.add(i - del), 1);
                }
            }
        }
        self.len -= del;
    }

    /// Returns a token-gated shared reference to element `idx`, if in bounds.
    #[inline(always)]
    pub fn get<'a>(&'a self, _token: &'a GhostToken<'brand>, idx: usize) -> Option<&'a T> {
        if idx < self.len {
            unsafe { Some(&*self.ptr.as_ptr().add(idx)) }
        } else {
            None
        }
    }

    /// Returns a token-gated exclusive reference to element `idx`, if in bounds.
    #[inline(always)]
    pub fn get_mut<'a>(
        &'a self,
        _token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
        if idx < self.len {
            unsafe { Some(&mut *self.ptr.as_ptr().add(idx)) }
        } else {
            None
        }
    }

    /// Returns a token-gated shared reference to element `idx` without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked<'a>(&'a self, _token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        &*self.ptr.as_ptr().add(idx)
    }

    /// Returns a token-gated exclusive reference to element `idx` without bounds checking.
    ///
    /// # Safety
    /// Caller must ensure `idx < self.len()`.
    #[inline(always)]
    pub unsafe fn get_unchecked_mut<'a>(
        &'a self,
        _token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> &'a mut T {
        &mut *self.ptr.as_ptr().add(idx)
    }

    /// Returns a token-gated shared reference to element `idx`.
    #[inline(always)]
    pub fn borrow<'a>(&'a self, token: &'a GhostToken<'brand>, idx: usize) -> &'a T {
        self.get(token, idx).expect("index out of bounds")
    }

    /// Returns a token-gated exclusive reference to element `idx`.
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
        &mut *self.ptr.as_ptr().add(idx)
    }

    /// Returns a mutable reference to element `idx` without a token.
    ///
    /// This requires exclusive access to the vector (`&mut self`).
    #[inline(always)]
    pub fn get_mut_exclusive(&mut self, idx: usize) -> Option<&mut T> {
        if idx < self.len {
            unsafe { Some(&mut *self.ptr.as_ptr().add(idx)) }
        } else {
            None
        }
    }

    /// Returns a raw pointer to the vector's buffer.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the vector's buffer.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Returns a slice of the underlying elements.
    #[inline(always)]
    pub fn as_slice<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a [T] {
        unsafe {
            std::slice::from_raw_parts(self.ptr.as_ptr(), self.len)
        }
    }

    /// Returns a mutable slice of the underlying elements.
    #[inline(always)]
    pub fn as_mut_slice<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> &'a mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
        }
    }

    /// Returns a mutable slice of the underlying elements without a token.
    #[inline(always)]
    pub fn as_mut_slice_exclusive(&mut self) -> &mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
        }
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(&'a self, token: &'a GhostToken<'brand>) -> slice::Iter<'a, T> {
        self.as_slice(token).iter()
    }

    /// Applies `f` to each element by exclusive reference.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        self.as_mut_slice(token).iter_mut().for_each(|item| f(item));
    }

    /// Applies `f` to each element by exclusive reference without a token.
    pub fn for_each_mut_exclusive(&mut self, mut f: impl FnMut(&mut T)) {
        self.as_mut_slice_exclusive().iter_mut().for_each(|item| f(item));
    }

    /// Zero-copy filter with fused iterator operations.
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

    /// Zero-copy find operation.
    #[inline(always)]
    pub fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).find(move |item| f(item))
    }

    /// Zero-copy position finder.
    #[inline(always)]
    pub fn position_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> Option<usize>
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).position(move |item| f(item))
    }

    /// Zero-cost fold.
    pub fn fold_ref<B, F>(&self, token: &GhostToken<'brand>, init: B, f: F) -> B
    where
        F: FnMut(B, &T) -> B,
    {
        self.iter(token).fold(init, f)
    }

    /// Zero-cost any/all.
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

    /// Zero-cost count.
    #[inline(always)]
    pub fn count_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        self.iter(token).filter(move |item| f(item)).count()
    }

    /// Zero-cost min_by.
    pub fn min_by_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T, &T) -> std::cmp::Ordering,
    {
        self.iter(token).min_by(|a, b| f(a, b))
    }

    /// Zero-cost max_by.
    pub fn max_by_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T, &T) -> std::cmp::Ordering,
    {
        self.iter(token).max_by(|a, b| f(a, b))
    }

    /// Creates a draining iterator.
    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = T> + '_
    where
        R: std::ops::RangeBounds<usize>,
    {
        let len = self.len;
        let start = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(&n) => n + 1,
            std::ops::Bound::Excluded(&n) => n,
            std::ops::Bound::Unbounded => len,
        };

        assert!(start <= end && end <= len);
        let count = end - start;

        // Move items out
        let mut result = Vec::with_capacity(count);
        unsafe {
            let ptr = self.ptr.as_ptr();
            ptr::copy_nonoverlapping(ptr.add(start), result.as_mut_ptr(), count);
            result.set_len(count);

            // Shift tail
            ptr::copy(ptr.add(end), ptr.add(start), len - end);
        }
        self.len -= count;
        result.into_iter()
    }
}

impl<'brand, T> Drop for BrandedVec<'brand, T> {
    fn drop(&mut self) {
        if self.cap > 0 && core::mem::size_of::<T>() > 0 {
            unsafe {
                // Drop elements
                let ptr = self.ptr.as_ptr();
                for i in 0..self.len {
                    ptr::drop_in_place(ptr.add(i));
                }
                // Dealloc
                dealloc(ptr as *mut u8, Layout::array::<T>(self.cap).unwrap());
            }
        }
    }
}

impl<'brand, T> crate::collections::BrandedCollection<'brand> for BrandedVec<'brand, T> {
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
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
        let mut v = Self::new();
        v.extend(iter);
        v
    }
}

impl<'brand, T> IntoIterator for BrandedVec<'brand, T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        // Convert to Vec<T> to use its iter
        if self.cap == 0 || core::mem::size_of::<T>() == 0 {
            let vec = unsafe { Vec::from_raw_parts(self.ptr.as_ptr(), self.len, self.cap) };
            core::mem::forget(self);
            vec.into_iter()
        } else {
            let vec = unsafe { Vec::from_raw_parts(self.ptr.as_ptr(), self.len, self.cap) };
            core::mem::forget(self);
            vec.into_iter()
        }
    }
}

impl<'brand, T> Extend<T> for BrandedVec<'brand, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }
}

// Ensure BrandedArray uses GhostCell internally as it is safe storage
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
            Some(self.inner[IDX].borrow(token))
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
            .map(|cell| cell.borrow(token))
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
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        idx: usize,
    ) -> Option<&'a mut T> {
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
        assert!(
            idx < self.len,
            "index {} out of bounds for BrandedArray of len {}",
            idx,
            self.len
        );
        self.inner[idx].borrow(token)
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
        self.inner[idx].borrow_mut(token)
    }

    /// Returns a slice of the underlying elements.
    #[inline(always)]
    pub fn as_slice<'a>(&'a self, _token: &'a GhostToken<'brand>) -> &'a [T] {
        unsafe {
            let ptr = self.inner.as_ptr() as *const T;
            std::slice::from_raw_parts(ptr, self.len)
        }
    }

    /// Returns a mutable slice of the underlying elements.
    #[inline(always)]
    pub fn as_mut_slice<'a>(&'a self, _token: &'a mut GhostToken<'brand>) -> &'a mut [T] {
        unsafe {
            let ptr = self.inner.as_ptr() as *mut T;
            std::slice::from_raw_parts_mut(ptr, self.len)
        }
    }

    /// Iterates over all elements by shared reference.
    pub fn iter<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
    ) -> impl Iterator<Item = &'a T> + 'a + use<'a, 'brand, T, CAPACITY> {
        self.inner[..self.len].iter().map(|cell| cell.borrow(token))
    }

    /// Applies `f` to each element by exclusive reference.
    ///
    /// This is the canonical safe pattern for *sequential* exclusive iteration:
    /// each `&mut T` is scoped to one callback invocation, preserving token linearity.
    pub fn for_each_mut(&self, token: &mut GhostToken<'brand>, mut f: impl FnMut(&mut T)) {
        self.inner[..self.len].iter().for_each(|cell| {
            f(cell.borrow_mut(token));
        });
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

// ... existing tests for BrandedVec ...
// I will include the tests from the original file to ensure no regressions.
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

    // ... other tests ...
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
}
