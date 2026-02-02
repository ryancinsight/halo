use halo::{concurrency::worklist::GhostChaseLevDeque, GhostToken};

#[test]
fn chase_lev_single_thread_push_pop() {
    GhostToken::new(|token| {
        let d: GhostChaseLevDeque<'_> = GhostChaseLevDeque::new(64);
        assert!(d.push_bottom(&token, 1));
        assert!(d.push_bottom(&token, 2));
        assert!(d.push_bottom(&token, 3));
        assert_eq!(d.pop_bottom(&token), Some(3));
        assert_eq!(d.pop_bottom(&token), Some(2));
        assert_eq!(d.pop_bottom(&token), Some(1));
        assert_eq!(d.pop_bottom(&token), None);
    });
}

#[test]
fn chase_lev_steal_from_other_thread() {
    GhostToken::new(|token| {
        let d: GhostChaseLevDeque<'_> = GhostChaseLevDeque::new(64);
        for i in 0..16usize {
            assert!(d.push_bottom(&token, i));
        }
        let steal_token = token.split_immutable().0;

        std::thread::scope(|s| {
            let d = &d;
            let steal_token = steal_token;
            let h = s.spawn(move || {
                let mut got = Vec::new();
                loop {
                    match d.steal(&steal_token) {
                        Some(x) => got.push(x),
                        None => break,
                    }
                }
                got
            });

            let stolen = h.join().unwrap();
            let mut remaining = Vec::new();
            while let Some(x) = d.pop_bottom(&token) {
                remaining.push(x);
            }

            let mut seen = [false; 16];
            for x in stolen.into_iter().chain(remaining) {
                assert!(x < 16);
                assert!(!seen[x], "duplicate item {x}");
                seen[x] = true;
            }
            assert!(seen.into_iter().all(|b| b));
        });
    });
}
