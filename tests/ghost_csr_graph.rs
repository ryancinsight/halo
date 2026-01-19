use halo::GhostToken;

#[test]
fn ghost_csr_graph_dfs_visits_expected_set() {
    GhostToken::new(|_token| {
        // 0 -> 1,2 ; 1 -> 3 ; 2 -> 3 ; 3 -> (none)
        let adj = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let g = halo::GhostCsrGraph::<256>::from_adjacency(&adj);

        g.reset_visited();
        let order = g.dfs(0);

        // Deterministic order with our reverse-push: 0,1,3,2
        assert_eq!(order, vec![0, 1, 3, 2]);

        // And the visited set is complete.
        for i in 0..4 {
            assert!(g.is_visited(i));
        }

        g.reset_visited();
        assert_eq!(g.dfs_count(0), 4);
    });
}
