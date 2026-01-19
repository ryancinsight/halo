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

use crate::{GhostCell, GhostToken};
use crate::collections::vec::{BrandedVec, slice::BrandedSlice, slice::BrandedSliceMut};
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
/// rectangular region of the matrix. It holds `&mut GhostCell` references implicitly
/// via raw pointers, but the API ensures safety and non-aliasing.
pub struct BrandedMatrixViewMut<'a, 'brand, T> {
    /// Pointer to the top-left element of this view in the original matrix.
    ptr: *mut GhostCell<'brand, T>,
    /// Number of rows in this view.
    rows: usize,
    /// Number of columns in this view.
    cols: usize,
    /// The stride (row pitch) of the underlying storage (items per row).
    stride: usize,
    /// Lifetime marker for the mutable borrow of the cells.
    _marker: PhantomData<&'a mut GhostCell<'brand, T>>,
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
        assert_eq!(vec.len(), rows * cols, "Vector length must match dimensions");
        Self { data: vec, rows, cols }
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
    pub fn get<'a>(&'a self, token: &'a GhostToken<'brand>, row: usize, col: usize) -> Option<&'a T> {
        if row < self.rows && col < self.cols {
            // SAFETY: bounds checked above.
            unsafe {
                Some(self.data.get_unchecked(token, row * self.cols + col))
            }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the element at (row, col).
    #[inline(always)]
    pub fn get_mut<'a>(&'a self, token: &'a mut GhostToken<'brand>, row: usize, col: usize) -> Option<&'a mut T> {
        if row < self.rows && col < self.cols {
            // SAFETY: bounds checked above.
            unsafe {
                Some(self.data.get_unchecked_mut(token, row * self.cols + col))
            }
        } else {
            None
        }
    }

    /// Returns a row as a `BrandedSlice`.
    pub fn row<'a>(&'a self, token: &'a GhostToken<'brand>, row: usize) -> Option<BrandedSlice<'a, 'brand, T>> {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            // Access inner vector directly safely
            let slice = &self.data.inner[start..end];
            Some(BrandedSlice::new(slice, token))
        } else {
            None
        }
    }

    /// Returns a mutable row as a `BrandedSliceMut`.
    ///
    /// This gives exclusive access to the row without needing `&mut GhostToken` if you have `&mut self`.
    /// But wait, `&mut self` gives full exclusivity.
    /// If you want to use `&mut GhostToken` with `&self`, we can't easily return `BrandedSliceMut` because `BrandedSliceMut` implies we have `&mut GhostCell`.
    /// `BrandedVec::get_mut` requires `&mut GhostToken` and returns `&mut T`.
    /// `BrandedSliceMut` requires `&mut [GhostCell]`. We only get `&mut [GhostCell]` from `&mut BrandedVec`.
    pub fn row_mut_exclusive<'a>(&'a mut self, row: usize) -> Option<BrandedSliceMut<'a, 'brand, T>> {
        if row < self.rows {
            let start = row * self.cols;
            let end = start + self.cols;
            let slice = &mut self.data.inner[start..end];
            Some(BrandedSliceMut::new(slice))
        } else {
            None
        }
    }

    /// Returns a view of the entire matrix for splitting.
    pub fn view_mut<'a>(&'a mut self) -> BrandedMatrixViewMut<'a, 'brand, T> {
        BrandedMatrixViewMut {
            ptr: self.data.inner.as_mut_ptr(),
            rows: self.rows,
            cols: self.cols,
            stride: self.cols,
            _marker: PhantomData,
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
                let cell = &mut *self.ptr.add(row * self.stride + col);
                Some(cell.get_mut())
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
            };
            let bottom = Self {
                ptr: self.ptr.add(mid * self.stride),
                rows: bottom_rows,
                cols: self.cols,
                stride: self.stride,
                _marker: PhantomData,
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
            };
            let right = Self {
                ptr: self.ptr.add(mid),
                rows: self.rows,
                cols: right_cols,
                stride: self.stride,
                _marker: PhantomData,
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
    /// Note: This is only possible if the view represents full contiguous rows (stride == cols).
    /// If stride != cols (i.e., it's a sub-column view), we cannot return a contiguous slice for rows
    /// without strided iterator support, which `BrandedSliceMut` does not support.
    ///
    /// However, we can return an iterator that yields mutable references to elements.
    pub fn for_each_mut<F>(self, mut f: F)
    where
        F: FnMut(usize, usize, &mut T),
    {
        for r in 0..self.rows {
            for c in 0..self.cols {
                unsafe {
                    let cell = &mut *self.ptr.add(r * self.stride + c);
                    f(r, c, cell.get_mut());
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
            assert_eq!(tl.rows(), 2); assert_eq!(tl.cols(), 2);
            assert_eq!(tr.rows(), 2); assert_eq!(tr.cols(), 2);

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
}
