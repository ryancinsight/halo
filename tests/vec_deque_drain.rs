use halo::{BrandedVecDeque, GhostToken};

#[test]
fn test_drain_middle() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::with_capacity(10);
        for i in 0..10 {
            dq.push_back(i);
        }

        // Drain [3, 4, 5, 6]
        let drained: Vec<_> = dq.drain(3..7).collect();
        assert_eq!(drained, vec![3, 4, 5, 6]);

        let remaining: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(remaining, vec![0, 1, 2, 7, 8, 9]);
    });
}

#[test]
fn test_drain_start() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..10);

        let drained: Vec<_> = dq.drain(..3).collect();
        assert_eq!(drained, vec![0, 1, 2]);

        let remaining: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(remaining, vec![3, 4, 5, 6, 7, 8, 9]);
    });
}

#[test]
fn test_drain_end() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..10);

        let drained: Vec<_> = dq.drain(7..).collect();
        assert_eq!(drained, vec![7, 8, 9]);

        let remaining: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(remaining, vec![0, 1, 2, 3, 4, 5, 6]);
    });
}

#[test]
fn test_drain_all() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..10);

        let drained: Vec<_> = dq.drain(..).collect();
        assert_eq!(drained, (0..10).collect::<Vec<_>>());
        assert!(dq.is_empty());
    });
}

#[test]
fn test_drain_partial() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..10);

        {
            let mut drain = dq.drain(3..7);
            assert_eq!(drain.next(), Some(3));
            // Drop drain without consuming rest
        }

        let remaining: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(remaining, vec![0, 1, 2, 7, 8, 9]);
    });
}


#[test]
fn test_splice_replace_same_len() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..5);
        // [0, 1, 2, 3, 4]
        // Splice 1..4 (vals 1, 2, 3) with [10, 20, 30]
        let spliced: Vec<_> = dq.splice(1..4, vec![10, 20, 30]).collect();
        assert_eq!(spliced, vec![1, 2, 3]);

        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![0, 10, 20, 30, 4]);
    });
}

#[test]
fn test_splice_replace_smaller() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..5);
        // [0, 1, 2, 3, 4]
        // Splice 1..4 (vals 1, 2, 3) with [10]
        let spliced: Vec<_> = dq.splice(1..4, vec![10]).collect();
        assert_eq!(spliced, vec![1, 2, 3]);

        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![0, 10, 4]);
    });
}

#[test]
fn test_splice_replace_larger() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..5);
        // [0, 1, 2, 3, 4]
        // Splice 1..2 (val 1) with [10, 11, 12]
        let spliced: Vec<_> = dq.splice(1..2, vec![10, 11, 12]).collect();
        assert_eq!(spliced, vec![1]);

        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![0, 10, 11, 12, 2, 3, 4]);
    });
}


#[test]
fn test_rotation() {
    GhostToken::new(|token| {
        let mut dq = BrandedVecDeque::from_iter(0..5);
        // [0, 1, 2, 3, 4]

        dq.rotate_left(2);
        // [2, 3, 4, 0, 1]
        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![2, 3, 4, 0, 1]);

        dq.rotate_right(1);
        // [1, 2, 3, 4, 0]
        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![1, 2, 3, 4, 0]);

        // Full wrap
        dq.rotate_left(4);
        // [0, 1, 2, 3, 4]
        let result: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    });
}
