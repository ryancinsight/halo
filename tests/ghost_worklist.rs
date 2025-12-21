use core::sync::atomic::{AtomicUsize, Ordering};

use halo::{concurrency::worklist::GhostTreiberStack, GhostToken};

#[test]
fn treiber_stack_single_thread_lifo() {
    GhostToken::new(|_token| {
        let s: GhostTreiberStack<'_> = GhostTreiberStack::new(8);
        s.push(1);
        s.push(2);
        s.push(3);
        assert_eq!(s.pop(), Some(3));
        assert_eq!(s.pop(), Some(2));
        assert_eq!(s.pop(), Some(1));
        assert_eq!(s.pop(), None);
    });
}

#[test]
fn treiber_stack_push_batch() {
    GhostToken::new(|_token| {
        let s: GhostTreiberStack<'_> = GhostTreiberStack::new(16);
        s.push_batch(&[1, 2, 3, 4]);
        assert_eq!(s.pop(), Some(1));
        assert_eq!(s.pop(), Some(2));
        assert_eq!(s.pop(), Some(3));
        assert_eq!(s.pop(), Some(4));
        assert_eq!(s.pop(), None);
    });
}

#[test]
fn treiber_stack_multi_thread_unique_push_pop() {
    const N: usize = 1024;
    GhostToken::new(|_token| {
        let s: GhostTreiberStack<'_> = GhostTreiberStack::new(N);
        let popped = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let s = &s;
            // Push all nodes from multiple threads (disjoint ranges).
            for t in 0..4 {
                scope.spawn(move || {
                    let start = t * (N / 4);
                    let end = (t + 1) * (N / 4);
                    for i in start..end {
                        s.push(i);
                    }
                });
            }
        });

        std::thread::scope(|scope| {
            let s = &s;
            for _ in 0..4 {
                scope.spawn(|| {
                    while s.pop().is_some() {
                        popped.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });

        assert_eq!(popped.load(Ordering::Relaxed), N);
        assert_eq!(s.pop(), None);
    });
}


