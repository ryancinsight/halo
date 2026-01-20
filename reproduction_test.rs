use halo::{BrandedVecDeque, GhostToken};

fn main() {
    GhostToken::new(|mut token| {
        let mut dq = BrandedVecDeque::with_capacity(10);
        for i in 0..10 {
            dq.push_back(i);
        }

        // Drain middle [3, 4, 5, 6]
        // Indices 3..7
        let drained: Vec<_> = dq.drain(3..7).collect();

        assert_eq!(drained, vec![3, 4, 5, 6]);

        let remaining: Vec<_> = dq.iter(&token).copied().collect();
        assert_eq!(remaining, vec![0, 1, 2, 7, 8, 9]);

        println!("Drain test passed!");
    });
}
