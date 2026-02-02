//! Active wrappers for `other` collections.

use super::{
    BrandedBinaryHeap, BrandedDeque, BrandedDisjointSet, BrandedDoublyLinkedList, BrandedFenwickTree,
    BrandedSegmentTree, TripodList,
};
use crate::token::traits::GhostBorrowMut;
use core::cmp::Ord;
use core::ops::{AddAssign, SubAssign};

/// A wrapper around a mutable reference to a `BrandedDoublyLinkedList` and a mutable reference to a `GhostToken`.
pub struct ActiveDoublyLinkedList<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    list: &'a mut BrandedDoublyLinkedList<'brand, T>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, Token> ActiveDoublyLinkedList<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active list handle.
    pub fn new(
        list: &'a mut BrandedDoublyLinkedList<'brand, T>,
        token: &'a mut Token,
    ) -> Self {
        Self { list, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Clears the list.
    pub fn clear(&mut self) {
        self.list.clear(self.token);
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) -> usize {
        self.list.push_front(self.token, value)
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) -> usize {
        self.list.push_back(self.token, value)
    }

    /// Pops an element from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.list.pop_front(self.token)
    }

    /// Pops an element from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.list.pop_back(self.token)
    }

    /// Returns a shared reference to the front element.
    pub fn front(&self) -> Option<&T> {
        self.list.front(self.token)
    }

    /// Returns a shared reference to the back element.
    pub fn back(&self) -> Option<&T> {
        self.list.back(self.token)
    }

    /// Returns a shared reference to the element at the given index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.list.get(self.token, index)
    }

    /// Returns a mutable reference to the element at the given index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.list.get_mut(self.token, index)
    }

    /// Iterates over the list elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ + use<'_, 'brand, T, Token> {
        self.list.iter(self.token)
    }

    /// Iterates over the list elements mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ + use<'_, 'brand, T, Token> {
        self.list.iter_mut(self.token)
    }

    /// Moves the node at `index` to the front.
    pub fn move_to_front(&mut self, index: usize) {
        self.list.move_to_front(self.token, index)
    }

    /// Moves the node at `index` to the back.
    pub fn move_to_back(&mut self, index: usize) {
        self.list.move_to_back(self.token, index)
    }
}

/// Extension trait to easily create ActiveDoublyLinkedList.
pub trait ActivateDoublyLinkedList<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDoublyLinkedList<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T> ActivateDoublyLinkedList<'brand, T> for BrandedDoublyLinkedList<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDoublyLinkedList<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveDoublyLinkedList::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `TripodList` and a mutable reference to a `GhostToken`.
pub struct ActiveTripodList<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    list: &'a mut TripodList<'brand, T>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, Token> ActiveTripodList<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active list handle.
    pub fn new(list: &'a mut TripodList<'brand, T>, token: &'a mut Token) -> Self {
        Self { list, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) -> usize {
        self.list.push_front(self.token, value)
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) -> usize {
        self.list.push_back(self.token, value)
    }

    /// Pops an element from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.list.pop_front(self.token)
    }

    /// Pops an element from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.list.pop_back(self.token)
    }

    /// Returns a reference to the front element.
    pub fn front(&self) -> Option<&T> {
        self.list.front(self.token)
    }

    /// Returns a reference to the back element.
    pub fn back(&self) -> Option<&T> {
        self.list.back(self.token)
    }

    /// Gets the parent index of a node at `index`.
    pub fn get_parent(&self, index: usize) -> Option<usize> {
        self.list.get_parent(self.token, index)
    }

    /// Sets the parent index of a node at `index`.
    pub fn set_parent(&mut self, index: usize, parent: Option<usize>) {
        self.list.set_parent(self.token, index, parent)
    }

    /// Iterates over the list.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ + use<'_, 'brand, T, Token> {
        self.list.iter(self.token)
    }

    /// Iterates over the list mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ + use<'_, 'brand, T, Token> {
        self.list.iter_mut(self.token)
    }
}

/// Extension trait to easily create ActiveTripodList.
pub trait ActivateTripodList<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveTripodList<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T> ActivateTripodList<'brand, T> for TripodList<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveTripodList<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveTripodList::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedBinaryHeap` and a mutable reference to a `GhostToken`.
pub struct ActiveBinaryHeap<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    heap: &'a mut BrandedBinaryHeap<'brand, T>,
    token: &'a mut Token,
}

impl<'a, 'brand, T: Ord, Token> ActiveBinaryHeap<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active heap handle.
    pub fn new(
        heap: &'a mut BrandedBinaryHeap<'brand, T>,
        token: &'a mut Token,
    ) -> Self {
        Self { heap, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Returns the capacity.
    pub fn capacity(&self) -> usize {
        self.heap.capacity()
    }

    /// Pushes an item.
    pub fn push(&mut self, item: T) {
        self.heap.push(self.token, item)
    }

    /// Pops the greatest item.
    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop(self.token)
    }

    /// Returns a reference to the greatest item.
    pub fn peek(&self) -> Option<&T> {
        self.heap.peek(self.token)
    }

    /// Clears the heap.
    pub fn clear(&mut self) {
        self.heap.clear()
    }

    /// Iterates over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ + use<'_, 'brand, T, Token> {
        self.heap.iter(self.token)
    }
}

/// Extension trait to easily create ActiveBinaryHeap from BrandedBinaryHeap.
pub trait ActivateBinaryHeap<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveBinaryHeap<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T: Ord> ActivateBinaryHeap<'brand, T> for BrandedBinaryHeap<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveBinaryHeap<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveBinaryHeap::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedDeque` (fixed size ring buffer) and a mutable reference to a `GhostToken`.
pub struct ActiveDeque<'a, 'brand, T, const CAPACITY: usize, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    deque: &'a mut BrandedDeque<'brand, T, CAPACITY>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, const CAPACITY: usize, Token> ActiveDeque<'a, 'brand, T, CAPACITY, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active deque handle.
    pub fn new(
        deque: &'a mut BrandedDeque<'brand, T, CAPACITY>,
        token: &'a mut Token,
    ) -> Self {
        Self { deque, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.deque.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    /// Returns `true` if full.
    pub fn is_full(&self) -> bool {
        self.deque.is_full()
    }

    /// Clears the deque.
    pub fn clear(&mut self) {
        self.deque.clear();
    }

    /// Pushes an element to the back.
    pub fn push_back(&mut self, value: T) -> Option<()> {
        self.deque.push_back(value)
    }

    /// Pushes an element to the front.
    pub fn push_front(&mut self, value: T) -> Option<()> {
        self.deque.push_front(value)
    }

    /// Pops from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.deque.pop_back().map(|c| c.into_inner())
    }

    /// Pops from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.deque.pop_front().map(|c| c.into_inner())
    }

    /// Returns the front element.
    pub fn front(&self) -> Option<&T> {
        self.deque.front(self.token)
    }

    /// Returns the back element.
    pub fn back(&self) -> Option<&T> {
        self.deque.back(self.token)
    }

    /// Returns a shared reference to the element at `idx`.
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.deque.get(self.token, idx)
    }

    /// Returns a mutable reference to the element at `idx`.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        self.deque.get_mut(self.token, idx)
    }

    /// Iterates over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> + ExactSizeIterator + '_ + use<'_, 'brand, T, CAPACITY, Token> {
        self.deque.iter(self.token)
    }

    /// Bulk operation.
    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(&T),
    {
        self.deque.for_each(self.token, f)
    }

    /// Bulk mutation.
    pub fn for_each_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T),
    {
        self.deque.for_each_mut(self.token, f)
    }
}

/// Extension trait to easily create ActiveDeque from BrandedDeque.
pub trait ActivateDeque<'brand, T, const CAPACITY: usize> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDeque<'a, 'brand, T, CAPACITY, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T, const CAPACITY: usize> ActivateDeque<'brand, T, CAPACITY>
    for BrandedDeque<'brand, T, CAPACITY>
{
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDeque<'a, 'brand, T, CAPACITY, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveDeque::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedFenwickTree` and a mutable reference to a `GhostToken`.
pub struct ActiveFenwickTree<'a, 'brand, T, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    tree: &'a mut BrandedFenwickTree<'brand, T>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, Token> ActiveFenwickTree<'a, 'brand, T, Token>
where
    T: Default + Copy + AddAssign + SubAssign,
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active Fenwick Tree handle.
    pub fn new(
        tree: &'a mut BrandedFenwickTree<'brand, T>,
        token: &'a mut Token,
    ) -> Self {
        Self { tree, token }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Adds `delta` to the element at `index`.
    pub fn add(&mut self, index: usize, delta: T) {
        self.tree.add(self.token, index, delta)
    }

    /// Computes prefix sum.
    pub fn prefix_sum(&self, index: usize) -> T {
        self.tree.prefix_sum(self.token, index)
    }

    /// Computes range sum.
    pub fn range_sum(&self, start: usize, end: usize) -> T {
        self.tree.range_sum(self.token, start, end)
    }

    /// Pushes a new value.
    pub fn push(&mut self, val: T) {
        self.tree.push(self.token, val)
    }

    /// Clears the tree.
    pub fn clear(&mut self) {
        self.tree.clear()
    }
}

/// Extension trait to easily create ActiveFenwickTree.
pub trait ActivateFenwickTree<'brand, T> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveFenwickTree<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T> ActivateFenwickTree<'brand, T> for BrandedFenwickTree<'brand, T>
where
    T: Default + Copy + AddAssign + SubAssign,
{
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveFenwickTree<'a, 'brand, T, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveFenwickTree::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedDisjointSet` and a mutable reference to a `GhostToken`.
pub struct ActiveDisjointSet<'a, 'brand, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    set: &'a mut BrandedDisjointSet<'brand>,
    token: &'a mut Token,
}

impl<'a, 'brand, Token> ActiveDisjointSet<'a, 'brand, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active disjoint set handle.
    pub fn new(
        set: &'a mut BrandedDisjointSet<'brand>,
        token: &'a mut Token,
    ) -> Self {
        Self { set, token }
    }

    /// Creates a new set containing a single element.
    pub fn make_set(&mut self) -> usize {
        self.set.make_set(self.token)
    }

    /// Finds the representative of the set containing `id`.
    pub fn find(&mut self, id: usize) -> usize {
        self.set.find(self.token, id)
    }

    /// Unites the sets containing `id1` and `id2`.
    pub fn union(&mut self, id1: usize, id2: usize) -> bool {
        self.set.union(self.token, id1, id2)
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// Returns `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

/// Extension trait to easily create ActiveDisjointSet.
pub trait ActivateDisjointSet<'brand> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDisjointSet<'a, 'brand, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand> ActivateDisjointSet<'brand> for BrandedDisjointSet<'brand> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveDisjointSet<'a, 'brand, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveDisjointSet::new(self, token)
    }
}

/// A wrapper around a mutable reference to a `BrandedSegmentTree` and a mutable reference to a `GhostToken`.
pub struct ActiveSegmentTree<'a, 'brand, T, F, Token>
where
    Token: GhostBorrowMut<'brand>,
{
    tree: &'a mut BrandedSegmentTree<'brand, T, F>,
    token: &'a mut Token,
}

impl<'a, 'brand, T, F, Token> ActiveSegmentTree<'a, 'brand, T, F, Token>
where
    T: Clone + PartialEq,
    F: Fn(&T, &T) -> T,
    Token: GhostBorrowMut<'brand>,
{
    /// Creates a new active segment tree handle.
    pub fn new(
        tree: &'a mut BrandedSegmentTree<'brand, T, F>,
        token: &'a mut Token,
    ) -> Self {
        Self { tree, token }
    }

    /// Updates the value at `index`.
    pub fn update(&mut self, index: usize, value: T) {
        self.tree.update(self.token, index, value)
    }

    /// Queries the range `[q_start, q_end)`.
    pub fn query(&self, q_start: usize, q_end: usize) -> std::borrow::Cow<'_, T> {
        self.tree.query(self.token, q_start, q_end)
    }

    /// Repairs the tree consistency.
    pub fn repair(&mut self) {
        self.tree.repair(self.token)
    }
}

/// Extension trait to easily create ActiveSegmentTree.
pub trait ActivateSegmentTree<'brand, T, F> {
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveSegmentTree<'a, 'brand, T, F, Token>
    where
        Token: GhostBorrowMut<'brand>;
}

impl<'brand, T, F> ActivateSegmentTree<'brand, T, F> for BrandedSegmentTree<'brand, T, F>
where
    T: Clone + PartialEq,
    F: Fn(&T, &T) -> T,
{
    fn activate<'a, Token>(
        &'a mut self,
        token: &'a mut Token,
    ) -> ActiveSegmentTree<'a, 'brand, T, F, Token>
    where
        Token: GhostBorrowMut<'brand>,
    {
        ActiveSegmentTree::new(self, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GhostToken;

    #[test]
    fn test_active_fenwick_tree() {
        GhostToken::new(|mut token| {
            let mut ft = BrandedFenwickTree::<i64>::new();
            let mut active = ft.activate(&mut token);

            for _ in 0..5 {
                active.push(0);
            }

            active.add(0, 10);
            active.add(2, 20);

            assert_eq!(active.prefix_sum(0), 10);
            assert_eq!(active.prefix_sum(2), 30);
            assert_eq!(active.range_sum(1, 3), 20);
        });
    }

    #[test]
    fn test_active_disjoint_set() {
        GhostToken::new(|mut token| {
            let mut ds = BrandedDisjointSet::new();
            let mut active = ds.activate(&mut token);

            let a = active.make_set();
            let b = active.make_set();

            assert!(active.union(a, b));
            assert_eq!(active.find(a), active.find(b));
        });
    }

    #[test]
    fn test_active_segment_tree() {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(4, |a, b| std::cmp::min(*a, *b), i32::MAX);
            let mut active = st.activate(&mut token);

            active.update(0, 10);
            active.update(1, 5);
            active.update(2, 20);
            active.update(3, 8);

            assert_eq!(*active.query(0, 4), 5);
            assert_eq!(*active.query(0, 2), 5);
            assert_eq!(*active.query(2, 4), 8);
        });
    }
}
