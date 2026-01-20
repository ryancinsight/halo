//! `BrandedMatrix` — a 2D dense matrix with token-gated access and sub-view capabilities.
//!
//! This implementation provides a contiguous 2D storage backed by `BrandedVec`, enabling
//! cache-friendly access patterns. It features hierarchical "sub-token" views (`BrandedMatrixViewMut`)
//! that allow safe splitting of the matrix into disjoint mutable regions for parallel processing.
//!
//! # Subtoken Hierarchy
//!
//! - `BrandedMatrix`: The owner of the data.
//! - `BrandedMatrixViewMut`: A mutable view into a sub-region (sub-matrix).
//!   This view acts as a "subtoken" that grants exclusive access to its specific cells
//!   without requiring the global `GhostToken`. This enables splitting the matrix recursively.

use crate::collections::vec::{slice::BrandedSlice, slice::BrandedSliceMut, BrandedVec};
use crate::token::InvariantLifetime;
use crate::GhostToken;
use std::marker::PhantomData;
use std::slice;

/// A branded 2D matrix.
pub struct BrandedMatrix<'brand, T> {
    data: BrandedVec<'brand, T>,
    rows: usize,
    cols: usize,
}

/// A mutable view into a sub-matrix.
///
/// This structure acts as a "sub-token" or capability, granting exclusive access to a
/// rectangular region of the matrix.
pub struct BrandedMatrixViewMut<'a, 'brand, T> {
    /// Pointer to the top-left element of this view in the original matrix.
    ptr: *mut T,
    /// Number of rows in this view.
    rows: usize,
    /// Number of columns in this view.
    cols: usize,
    /// The stride (row pitch) of the underlying storage (items per row).
    stride: usize,
    /// Lifetime marker for the mutable borrow of the cells.
    _marker: PhantomData<&'a mut T>,
    _brand: InvariantLifetime<'brand>,
}

unsafe impl<'a, 'brand, T: Send> Send for BrandedMatrixViewMut<'a, 'brand, T> {}
unsafe impl<'a, 'brand, T: Sync> Sync for BrandedMatrixViewMut<'a, 'brand, T> {}

impl<'brand, T> BrandedMatrix<'brand, T> {
    /// Creates a new matrix with dimensions `rows x cols`, initialized with default values.
    pub fn new(rows: usize, cols: usize) -> Self
    where
        T: Default,
    {
        let mut data = BrandedVec::with_capacity(rows * cols);
        for _ in 0..(rows * cols) {
            data.push(T::default());
        }
        Self { data, rows, cols }
    }

    /// Creates a new matrix from a linear vector.
    ///
    /// # Panics
    /// Panics if `vec.len() != rows * cols`.
    pub fn from_vec(vec: BrandedVec<'brand, T>, rows: usize, cols: usize) -> Self {
        assert_eq!(
            vec.len(),
            rows * cols,
            "Vector length must match dimensions"
        );
        Self {
            data: vec,
            rows,
            cols,
        }
    }

    /// Returns the number of rows.
    #[inline(always)]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns.
    #[inline(always)]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Returns a shared reference to the element at (row, col).
    #[inline(always)]
    pub fn get<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        row: usize,
        col: usize,
    ) -> Option<&'a T> {
        if row < self.rows && col < self.cols {
            // SAFETY: bounds checked above.
            unsafe { Some(self.data.get_unchecked(token, row * self.cols + col)) }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the element at (row, col).
    #[inline(always)]
    pub fn get_mut<'a>(
        &'a self,
        token: &'a mut GhostToken<'brand>,
        row: usize,
        col: usize,
    ) -> Option<&'a mut T> {
        if row < self.rows && col < self.cols {
            // SAFETY: bounds checked above.
            unsafe { Some(self.data.get_unchecked_mut(token, row * self.cols + col)) }
        } else {
            None
        }
    }

    /// Returns a row as a `BrandedSlice`.
    pub fn row<'a>(
        &'a self,
        token: &'a GhostToken<'brand>,
        row: usize,
    ) -> Option<BrandedSlice<'a, 'brand, T>> {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            let slice = &self.data.as_slice(token)[start..end];
            Some(BrandedSlice::new(slice, token))
        } else {
            None
        }
    }

    /// Returns a mutable row as a `BrandedSliceMut`.
    ///
    /// This gives exclusive access to the row without needing `&mut GhostToken` if you have `&mut self`.
    pub fn row_mut_exclusive<'a>(
        &'a mut self,
        row: usize,
    ) -> Option<BrandedSliceMut<'a, 'brand, T>> {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            let slice = &mut self.data.as_mut_slice_exclusive()[start..end];
            Some(BrandedSliceMut::new(slice))
        } else {
            None
        }
    }

    /// Returns a view of the entire matrix for splitting.
    pub fn view_mut<'a>(&'a mut self) -> BrandedMatrixViewMut<'a, 'brand, T> {
        BrandedMatrixViewMut {
            ptr: self.data.as_mut_ptr(),
            rows: self.rows,
            cols: self.cols,
            stride: self.cols,
            _marker: PhantomData,
            _brand: InvariantLifetime::default(),
        }
    }
}

impl<'a, 'brand, T> BrandedMatrixViewMut<'a, 'brand, T> {
    /// Returns the number of rows in this view.
    #[inline(always)]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns in this view.
    #[inline(always)]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Returns a mutable reference to the element at (row, col) within this view.
    #[inline(always)]
    pub fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut T> {
        if row < self.rows && col < self.cols {
            unsafe {
                Some(&mut *self.ptr.add(row * self.stride + col))
            }
        } else {
            None
        }
    }

    /// Splits the view horizontally at `mid` row.
    ///
    /// Returns `(top, bottom)`.
    pub fn split_at_row(self, mid: usize) -> (Self, Self) {
        assert!(mid <= self.rows);
        let top_rows = mid;
        let bottom_rows = self.rows - mid;

        unsafe {
            let top = Self {
                ptr: self.ptr,
                rows: top_rows,
                cols: self.cols,
                stride: self.stride,
                _marker: PhantomData,
                _brand: InvariantLifetime::default(),
            };
            let bottom = Self {
                ptr: self.ptr.add(mid * self.stride),
                rows: bottom_rows,
                cols: self.cols,
                stride: self.stride,
                _marker: PhantomData,
                _brand: InvariantLifetime::default(),
            };
            (top, bottom)
        }
    }

    /// Splits the view vertically at `mid` column.
    ///
    /// Returns `(left, right)`.
    pub fn split_at_col(self, mid: usize) -> (Self, Self) {
        assert!(mid <= self.cols);
        let left_cols = mid;
        let right_cols = self.cols - mid;

        unsafe {
            let left = Self {
                ptr: self.ptr,
                rows: self.rows,
                cols: left_cols,
                stride: self.stride,
                _marker: PhantomData,
                _brand: InvariantLifetime::default(),
            };
            let right = Self {
                ptr: self.ptr.add(mid),
                rows: self.rows,
                cols: right_cols,
                stride: self.stride,
                _marker: PhantomData,
                _brand: InvariantLifetime::default(),
            };
            (left, right)
        }
    }

    /// Splits the view into 4 quadrants at (mid_row, mid_col).
    ///
    /// Returns `(top_left, top_right, bottom_left, bottom_right)`.
    pub fn split_quadrants(self, mid_row: usize, mid_col: usize) -> (Self, Self, Self, Self) {
        let (top, bottom) = self.split_at_row(mid_row);
        let (tl, tr) = top.split_at_col(mid_col);
        let (bl, br) = bottom.split_at_col(mid_col);
        (tl, tr, bl, br)
    }

    /// Iterates over the rows of this view as `BrandedSliceMut`.
    ///
    /// This is possible because elements within a row are always contiguous in memory,
    /// even if the view represents a sub-set of columns.
    pub fn rows_mut<'b>(&'b mut self) -> impl Iterator<Item = BrandedSliceMut<'b, 'brand, T>> + 'b
    where
        'a: 'b,
    {
        // We iterate `rows` times.
        // Each time we return a BrandedSliceMut starting at `ptr + r*stride` with len `cols`.
        struct RowsMutIter<'b, 'brand, T> {
            ptr: *mut T,
            end_row_idx: usize,
            current_row_idx: usize,
            stride: usize,
            cols: usize,
            _marker: PhantomData<&'b mut T>,
            _brand: InvariantLifetime<'brand>,
        }

        impl<'b, 'brand, T> Iterator for RowsMutIter<'b, 'brand, T> {
            type Item = BrandedSliceMut<'b, 'brand, T>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.current_row_idx >= self.end_row_idx {
                    return None;
                }
                unsafe {
                    let row_start = self.ptr.add(self.current_row_idx * self.stride);
                    let slice = slice::from_raw_parts_mut(row_start, self.cols);
                    self.current_row_idx += 1;
                    Some(BrandedSliceMut::new(slice))
                }
            }
        }

        RowsMutIter {
            ptr: self.ptr,
            end_row_idx: self.rows,
            current_row_idx: 0,
            stride: self.stride,
            cols: self.cols,
            _marker: PhantomData,
            _brand: InvariantLifetime::default(),
        }
    }

    /// Fills the view with a value.
    ///
    /// Optimized to use `slice::fill` per row.
    pub fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        for mut row in self.rows_mut() {
            row.as_mut_slice().fill(value.clone());
        }
    }

    /// Iterates over the rows of this view as `BrandedSliceMut`.
    /// note: This provides a callback-based iteration which might be easier for some patterns.
    pub fn for_each_mut<F>(self, mut f: F)
    where
        F: FnMut(usize, usize, &mut T),
    {
        for r in 0..self.rows {
            for c in 0..self.cols {
                unsafe {
                    let cell = &mut *self.ptr.add(r * self.stride + c);
                    f(r, c, cell);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_basic() {
        GhostToken::new(|mut token| {
            let mut mat = BrandedMatrix::new(2, 2);
            // Default is 0 (i32 default)
            assert_eq!(*mat.get(&token, 0, 0).unwrap(), 0);

            *mat.get_mut(&mut token, 0, 0).unwrap() = 1;
            *mat.get_mut(&mut token, 0, 1).unwrap() = 2;
            *mat.get_mut(&mut token, 1, 0).unwrap() = 3;
            *mat.get_mut(&mut token, 1, 1).unwrap() = 4;

            assert_eq!(*mat.get(&token, 0, 0).unwrap(), 1);
            assert_eq!(*mat.get(&token, 1, 1).unwrap(), 4);
        });
    }

    #[test]
    fn test_matrix_view_split() {
        GhostToken::new(|mut token| {
            let mut mat = BrandedMatrix::new(4, 4);
            // Fill matrix
            let mut val = 0;
            for r in 0..4 {
                for c in 0..4 {
                    *mat.get_mut(&mut token, r, c).unwrap() = val;
                    val += 1;
                }
            }

            // Split into 4 quadrants
            let view = mat.view_mut();
            let (mut tl, mut tr, mut bl, mut br) = view.split_quadrants(2, 2);

            // Check dimensions
            assert_eq!(tl.rows(), 2);
            assert_eq!(tl.cols(), 2);
            assert_eq!(tr.rows(), 2);
            assert_eq!(tr.cols(), 2);

            // Mutate independently
            *tl.get_mut(0, 0).unwrap() += 100; // 0 -> 100
            *tr.get_mut(0, 0).unwrap() += 100; // 2 -> 102
            *bl.get_mut(0, 0).unwrap() += 100; // 8 -> 108
            *br.get_mut(0, 0).unwrap() += 100; // 10 -> 110

            // Verify in original matrix
            assert_eq!(*mat.get(&token, 0, 0).unwrap(), 100);
            assert_eq!(*mat.get(&token, 0, 2).unwrap(), 102);
            assert_eq!(*mat.get(&token, 2, 0).unwrap(), 108);
            assert_eq!(*mat.get(&token, 2, 2).unwrap(), 110);
        });
    }

    #[test]
    fn test_matrix_view_recursive_split() {
        GhostToken::new(|mut token| {
            let mut mat = BrandedMatrix::new(4, 1);
            let view = mat.view_mut();
            let (v1, v2) = view.split_at_row(2);
            let (mut v1a, mut v1b) = v1.split_at_row(1);

            *v1a.get_mut(0, 0).unwrap() = 10;
            *v1b.get_mut(0, 0).unwrap() = 20;

            assert_eq!(*mat.get(&token, 0, 0).unwrap(), 10);
            assert_eq!(*mat.get(&token, 1, 0).unwrap(), 20);
        });
    }

    #[test]
    fn test_matrix_rows_mut_and_fill() {
        GhostToken::new(|mut token| {
            let mut mat = BrandedMatrix::new(4, 4);
            let mut view = mat.view_mut();

            // Fill top half with 1
            let (mut top, mut bottom) = view.split_at_row(2);
            top.fill(1);

            // Fill bottom half with 2 via iterator
            for mut row in bottom.rows_mut() {
                for val in row.as_mut_slice() {
                    *val = 2;
                }
            }

            assert_eq!(*mat.get(&token, 0, 0).unwrap(), 1);
            assert_eq!(*mat.get(&token, 1, 3).unwrap(), 1);
            assert_eq!(*mat.get(&token, 2, 0).unwrap(), 2);
            assert_eq!(*mat.get(&token, 3, 3).unwrap(), 2);
        });
    }
}
