//! `BrandedSmallVec` — a small-vector optimization for token-gated collections.
//!
//! This collection stores elements inline (on the stack or within the containing struct)
//! for small sizes, avoiding heap allocation. When the capacity is exceeded, it spills
//! to a `BrandedVec` on the heap.
//!
//! This provides:
//! - **Zero allocation** for small collections (up to `N` elements)
//! - **Cache locality** by keeping data inline
//! - **Token-gated safety** via `GhostCell`
//! - **Zero-copy operations** via `ZeroCopyOps`

use crate::collections::{BrandedCollection, BrandedVec, ZeroCopyOps};
use crate::GhostCell;
use core::mem::MaybeUninit;
use core::ptr;

use crate::token::traits::{GhostBorrow, GhostBorrowMut};

/// A vector that stores up to `N` elements inline, spilling to the heap if necessary.
pub struct BrandedSmallVec<'brand, T, const N: usize> {
    inner: SmallVecInner<'brand, T, N>,
}

enum SmallVecInner<'brand, T, const N: usize> {
    Inline {
        len: usize,
        data: [MaybeUninit<GhostCell<'brand, T>>; N],
    },
    Heap(BrandedVec<'brand, T>),
}

impl<'brand, T, const N: usize> BrandedSmallVec<'brand, T, N> {
    /// Creates a new empty `BrandedSmallVec`.
    #[inline]
    pub fn new() -> Self {
        // SAFETY: An array of MaybeUninit is safe to create uninitialized.
        // We use a safe way to create the uninitialized array.
        let data = unsafe {
            MaybeUninit::<[MaybeUninit<GhostCell<'brand, T>>; N]>::uninit().assume_init()
        };
        Self {
            inner: SmallVecInner::Inline { len: 0, data },
        }
    }

    /// Returns the number of elements in the vector.
    #[inline]
    pub fn len(&self) -> usize {
        match &self.inner {
            SmallVecInner::Inline { len, .. } => *len,
            SmallVecInner::Heap(v) => v.len(),
        }
    }

    /// Returns `true` if the vector is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pushes a new element to the vector.
    ///
    /// Spills to heap if inline capacity `N` is exceeded.
    #[inline]
    pub fn push(&mut self, value: T) {
        match &mut self.inner {
            SmallVecInner::Inline { len, data } => {
                if *len < N {
                    // SAFETY: We checked capacity. Writing to MaybeUninit is safe.
                    data[*len].write(GhostCell::new(value));
                    *len += 1;
                } else {
                    // Spill to heap
                    let mut vec = BrandedVec::with_capacity(N * 2);
                    // Move existing elements
                    for i in 0..*len {
                        // SAFETY: elements 0..len are initialized
                        let cell = unsafe { data[i].assume_init_read() };
                        vec.push(cell.into_inner());
                    }
                    vec.push(value);
                    self.inner = SmallVecInner::Heap(vec);
                }
            }
            SmallVecInner::Heap(v) => v.push(value),
        }
    }

    /// Pops the last element from the vector.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        match &mut self.inner {
            SmallVecInner::Inline { len, data } => {
                if *len > 0 {
                    *len -= 1;
                    // SAFETY: element at len was initialized (since we decremented first, it was at old len-1)
                    let cell = unsafe { data[*len].assume_init_read() };
                    Some(cell.into_inner())
                } else {
                    None
                }
            }
            SmallVecInner::Heap(v) => v.pop().map(|c| c.into_inner()),
        }
    }

    /// Returns a slice of the underlying elements.
    #[inline]
    pub fn as_slice<'a, Token>(&'a self, token: &'a Token) -> &'a [T]
    where
        Token: GhostBorrow<'brand>,
    {
        match &self.inner {
            SmallVecInner::Inline { len, data } => unsafe {
                // SAFETY: GhostCell<T> has the same layout as T, and MaybeUninit<U> has the same layout as U.
                // We have shared access via the token, so reading is safe.
                let ptr = data.as_ptr() as *const T;
                core::slice::from_raw_parts(ptr, *len)
            },
            SmallVecInner::Heap(v) => v.as_slice(token),
        }
    }

    /// Returns a mutable slice of the underlying elements.
    #[inline]
    pub fn as_mut_slice<'a, Token>(&'a self, token: &'a mut Token) -> &'a mut [T]
    where
        Token: GhostBorrowMut<'brand>,
    {
        match &self.inner {
            SmallVecInner::Inline { len, data } => unsafe {
                // SAFETY: GhostCell<T> has the same layout as T.
                // We have exclusive access via the mutable token, so writing is safe.
                let ptr = data.as_ptr() as *mut T;
                core::slice::from_raw_parts_mut(ptr, *len)
            },
            SmallVecInner::Heap(v) => v.as_mut_slice(token),
        }
    }

    /// Returns a shared reference to the element at `index`.
    #[inline]
    pub fn get<'a, Token>(&'a self, token: &'a Token, index: usize) -> Option<&'a T>
    where
        Token: GhostBorrow<'brand>,
    {
        self.as_slice(token).get(index)
    }

    /// Returns a mutable reference to the element at `index`.
    #[inline]
    pub fn get_mut<'a, Token>(
        &'a self,
        token: &'a mut Token,
        index: usize,
    ) -> Option<&'a mut T>
    where
        Token: GhostBorrowMut<'brand>,
    {
        self.as_mut_slice(token).get_mut(index)
    }

    /// Iterates over elements by shared reference.
    pub fn iter<'a, Token>(
        &'a self,
        token: &'a Token,
    ) -> impl Iterator<Item = &'a T> + ExactSizeIterator + DoubleEndedIterator + std::iter::FusedIterator + use<'a, 'brand, T, N, Token>
    where
        Token: GhostBorrow<'brand>,
    {
        self.as_slice(token).iter()
    }

    /// Returns true if the vector is spilled to the heap.
    pub fn is_spilled(&self) -> bool {
        match &self.inner {
            SmallVecInner::Heap(_) => true,
            _ => false,
        }
    }
}

impl<'brand, T, const N: usize> Default for BrandedSmallVec<'brand, T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'brand, T, const N: usize> Drop for BrandedSmallVec<'brand, T, N> {
    fn drop(&mut self) {
        match &mut self.inner {
            SmallVecInner::Inline { len, data } => {
                for i in 0..*len {
                    // SAFETY: elements 0..len are initialized
                    unsafe { ptr::drop_in_place(data[i].as_mut_ptr()) };
                }
            }
            SmallVecInner::Heap(_) => {
                // BrandedVec drops its contents automatically
            }
        }
    }
}

impl<'brand, T, const N: usize> BrandedCollection<'brand> for BrandedSmallVec<'brand, T, N> {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }
}

impl<'brand, T, const N: usize> ZeroCopyOps<'brand, T> for BrandedSmallVec<'brand, T, N> {
    fn find_ref<'a, F, Token>(&'a self, token: &'a Token, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
        Token: GhostBorrow<'brand>,
    {
        self.as_slice(token).iter().find(|&x| f(x))
    }

    fn any_ref<F, Token>(&self, token: &Token, f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: GhostBorrow<'brand>,
    {
        self.as_slice(token).iter().any(|x| f(x))
    }

    fn all_ref<F, Token>(&self, token: &Token, f: F) -> bool
    where
        F: Fn(&T) -> bool,
        Token: GhostBorrow<'brand>,
    {
        self.as_slice(token).iter().all(|x| f(x))
    }
}

impl<'brand, T, const N: usize> IntoIterator for BrandedSmallVec<'brand, T, N> {
    type Item = T;
    type IntoIter = BrandedSmallVecIntoIter<'brand, T, N>;

    fn into_iter(self) -> Self::IntoIter {
        // Move inner out to avoid dropping it
        let inner = unsafe { ptr::read(&self.inner) };
        core::mem::forget(self);

        match inner {
            SmallVecInner::Inline { len, data } => BrandedSmallVecIntoIter::Inline {
                data,
                index: 0,
                len,
            },
            SmallVecInner::Heap(vec) => BrandedSmallVecIntoIter::Heap(vec.into_iter()),
        }
    }
}

pub enum BrandedSmallVecIntoIter<'brand, T, const N: usize> {
    Inline {
        data: [MaybeUninit<GhostCell<'brand, T>>; N],
        index: usize,
        len: usize,
    },
    Heap(<BrandedVec<'brand, T> as IntoIterator>::IntoIter),
}

impl<'brand, T, const N: usize> Iterator for BrandedSmallVecIntoIter<'brand, T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            BrandedSmallVecIntoIter::Inline { len, data, index } => {
                if *index < *len {
                    // SAFETY: index checked against len.
                    // We are reading out the value, effectively moving it.
                    let cell = unsafe { data.get_unchecked(*index).assume_init_read() };
                    *index += 1;
                    Some(cell.into_inner())
                } else {
                    None
                }
            }
            BrandedSmallVecIntoIter::Heap(iter) => iter.next(),
        }
    }
}

impl<'brand, T, const N: usize> Drop for BrandedSmallVecIntoIter<'brand, T, N> {
    fn drop(&mut self) {
        match self {
            BrandedSmallVecIntoIter::Inline { len, data, index } => {
                while *index < *len {
                    unsafe { ptr::drop_in_place(data[*index].as_mut_ptr()) };
                    *index += 1;
                }
            }
            BrandedSmallVecIntoIter::Heap(_) => {
                // Iterator drops remaining elements
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_inline_storage() {
        GhostToken::new(|mut token| {
            let mut vec: BrandedSmallVec<'_, i32, 4> = BrandedSmallVec::new();
            assert!(vec.is_empty());
            assert!(!vec.is_spilled());

            vec.push(1);
            vec.push(2);
            vec.push(3);
            vec.push(4);

            assert_eq!(vec.len(), 4);
            assert!(!vec.is_spilled());

            assert_eq!(vec.get(&token, 0), Some(&1));
            assert_eq!(vec.get(&token, 3), Some(&4));

            *vec.get_mut(&mut token, 0).unwrap() = 10;
            assert_eq!(vec.get(&token, 0), Some(&10));

            assert_eq!(vec.pop(), Some(4));
            assert_eq!(vec.len(), 3);
        });
    }

    #[test]
    fn test_spill_to_heap() {
        GhostToken::new(|mut token| {
            let mut vec: BrandedSmallVec<'_, i32, 2> = BrandedSmallVec::new();

            vec.push(1);
            vec.push(2);
            assert!(!vec.is_spilled());

            vec.push(3);
            assert!(vec.is_spilled());
            assert_eq!(vec.len(), 3);

            assert_eq!(vec.get(&token, 0), Some(&1));
            assert_eq!(vec.get(&token, 2), Some(&3));

            assert_eq!(vec.pop(), Some(3));
            assert_eq!(vec.len(), 2);
            // It remains spilled (usually SmallVec doesn't shrink back automatically)
            assert!(vec.is_spilled());
        });
    }

    #[test]
    fn test_zero_copy_ops() {
        GhostToken::new(|token| {
            let mut vec: BrandedSmallVec<'_, i32, 4> = BrandedSmallVec::new();
            vec.push(10);
            vec.push(20);

            assert_eq!(vec.find_ref(&token, |&x| x == 20), Some(&20));
            assert!(vec.any_ref(&token, |&x| x > 15));
        });
    }

    #[test]
    fn test_into_iter() {
        GhostToken::new(|_token| {
            let mut vec: BrandedSmallVec<'_, i32, 4> = BrandedSmallVec::new();
            vec.push(1);
            vec.push(2);
            vec.push(3);

            let collected: Vec<i32> = vec.into_iter().collect();
            assert_eq!(collected, vec![1, 2, 3]);
        });
    }
}
