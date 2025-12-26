use halo::collections::ChunkedVec;

#[test]
fn chunked_vec_push_get_iter() {
    let mut v: ChunkedVec<u32, 8> = ChunkedVec::new();
    assert!(v.is_empty());

    for i in 0..100 {
        let idx = v.push(i);
        assert_eq!(idx, i as usize);
    }
    assert_eq!(v.len(), 100);

    assert_eq!(v.get(0), Some(&0));
    assert_eq!(v.get(99), Some(&99));
    assert_eq!(v.get(100), None);

    let sum: u32 = v.iter().copied().sum();
    assert_eq!(sum, (0..100u32).sum());
}

#[test]
fn chunked_vec_non_pow2() {
    let mut v: ChunkedVec<u32, 7> = ChunkedVec::new();
    for i in 0..100 {
        v.push(i);
    }
    assert_eq!(v.len(), 100);
    assert_eq!(v.get(99), Some(&99));
    let sum: u32 = v.iter().copied().sum();
    assert_eq!(sum, (0..100u32).sum());
}






