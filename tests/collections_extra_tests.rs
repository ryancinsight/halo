use halo::{GhostToken, GhostCell};
use halo::collections::vec::BrandedVec;
use halo::collections::vec::BrandedVecDeque;
use halo::collections::other::BrandedDoublyLinkedList;

#[test]
fn test_branded_vec_from_iter() {
    GhostToken::new(|token| {
        let vec: BrandedVec<_> = (0..5).collect();
        assert_eq!(vec.len(), 5);
        for i in 0..5 {
            assert_eq!(*vec.borrow(&token, i), i);
        }
    });
}

#[test]
fn test_branded_vec_into_iter() {
    GhostToken::new(|token| {
        let vec: BrandedVec<_> = (0..5).collect();
        let items: Vec<_> = vec.into_iter().collect();
        assert_eq!(items, vec![0, 1, 2, 3, 4]);
    });
}

#[test]
fn test_branded_vec_extend() {
    GhostToken::new(|token| {
        let mut vec = BrandedVec::new();
        vec.extend(0..3);
        assert_eq!(vec.len(), 3);
        assert_eq!(*vec.borrow(&token, 0), 0);
        assert_eq!(*vec.borrow(&token, 2), 2);
    });
}

#[test]
fn test_branded_vec_drain() {
    GhostToken::new(|mut token| {
        let mut vec: BrandedVec<_> = (0..5).collect();
        let drained: Vec<_> = vec.drain(1..4).collect();
        assert_eq!(drained, vec![1, 2, 3]);
        assert_eq!(vec.len(), 2);
        assert_eq!(*vec.borrow(&token, 0), 0);
        assert_eq!(*vec.borrow(&token, 1), 4);
    });
}

#[test]
fn test_branded_vec_deque_from_iter() {
    GhostToken::new(|token| {
        let deque: BrandedVecDeque<_> = (0..5).collect();
        assert_eq!(deque.len(), 5);
        assert_eq!(*deque.get(&token, 0).unwrap(), 0);
    });
}

#[test]
fn test_branded_vec_deque_into_iter() {
    GhostToken::new(|token| {
        let deque: BrandedVecDeque<_> = (0..5).collect();
        let items: Vec<_> = deque.into_iter().collect();
        assert_eq!(items, vec![0, 1, 2, 3, 4]);
    });
}

#[test]
fn test_branded_vec_deque_extend() {
    GhostToken::new(|token| {
        let mut deque = BrandedVecDeque::new();
        deque.extend(0..3);
        assert_eq!(deque.len(), 3);
        assert_eq!(*deque.get(&token, 0).unwrap(), 0);
    });
}

#[test]
fn test_branded_vec_deque_drain() {
    GhostToken::new(|mut token| {
        let mut deque: BrandedVecDeque<_> = (0..5).collect();
        let drained: Vec<_> = deque.drain(1..4).collect();
        assert_eq!(drained, vec![1, 2, 3]);
        assert_eq!(deque.len(), 2);
        assert_eq!(*deque.get(&token, 0).unwrap(), 0);
        assert_eq!(*deque.get(&token, 1).unwrap(), 4);
    });
}

// Removed test_branded_doubly_linked_list_from_iter/into_iter as they require token-free allocation which is not supported by BrandedPool.
