//! Tests for CSC graph implementation.

use super::*;
use crate::GhostToken;

#[test]
fn csc_graph_from_adjacency() {
    GhostToken::new(|_token| {
        // Create a simple graph: 0->1, 0->2, 1->2
        let adjacency = vec![
            vec![1, 2], // node 0
            vec![2],    // node 1
            vec![],     // node 2
        ];

        let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);

        assert_eq!(csc.node_count(), 3);
        assert_eq!(csc.edge_count(), 3);

        // Check in-neighbors (transpose edges)
        assert_eq!(csc.in_neighbors(0).collect::<Vec<_>>(), Vec::<usize>::new()); // no one points to 0
        assert_eq!(csc.in_neighbors(1).collect::<Vec<_>>(), vec![0]); // 0->1 becomes 0 in column 1
        assert_eq!(csc.in_neighbors(2).collect::<Vec<_>>(), vec![0, 1]); // 0->2 and 1->2 become [0,1] in column 2
    });
}

#[test]
fn csc_graph_dfs_traversal() {
    GhostToken::new(|_token| {
        // Create a graph: 0->1->2, 0->2
        let adjacency = vec![
            vec![1, 2], // node 0
            vec![2],    // node 1
            vec![],     // node 2
        ];

        let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
        let stack = GhostTreiberStack::new(10);

        // Incoming traversal: nodes that can reach `start` in the original graph.
        assert_eq!(csc.dfs_reachable_count(0, &stack), 1);
        assert_eq!(csc.dfs_reachable_count(2, &stack), 3);
    });
}

#[test]
fn csc_graph_bfs_traversal() {
    GhostToken::new(|_token| {
        // Create a graph: 0->1->2, 0->2
        let adjacency = vec![
            vec![1, 2], // node 0
            vec![2],    // node 1
            vec![],     // node 2
        ];

        let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
        let deque = GhostChaseLevDeque::new(32);

        assert_eq!(csc.bfs_reachable_count(0, &deque), 1);
        assert_eq!(csc.bfs_reachable_count(2, &deque), 3);
    });
}

#[test]
fn csc_graph_to_csr_conversion() {
    GhostToken::new(|_token| {
        let adjacency = vec![vec![1, 2], vec![2], vec![]];

        let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);
        let csr = csc.to_csr();

        assert_eq!(csr.node_count(), 3);
        assert_eq!(csr.edge_count(), 3);

        // Verify the conversion is correct
        assert_eq!(csr.neighbors(0).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(csr.neighbors(1).collect::<Vec<_>>(), vec![2]);
        assert_eq!(csr.neighbors(2).collect::<Vec<_>>(), Vec::<usize>::new());
    });
}

#[test]
fn csc_graph_degrees_and_membership() {
    GhostToken::new(|_token| {
        let adjacency = vec![vec![1, 2], vec![2], vec![]];

        let csc = GhostCscGraph::<1024>::from_adjacency(&adjacency);

        assert_eq!(csc.in_degree(0), 0); // no incoming edges
        assert_eq!(csc.in_degree(1), 1); // one incoming edge (0->1)
        assert_eq!(csc.in_degree(2), 2); // two incoming edges (0->2, 1->2)

        assert!(!csc.has_edge(0, 0)); // no self-loops
        assert!(csc.has_edge(0, 1)); // 0->1 exists
        assert!(csc.has_edge(0, 2)); // 0->2 exists
        assert!(csc.has_edge(1, 2)); // 1->2 exists
        assert!(!csc.has_edge(2, 0)); // 2->0 doesn't exist
    });
}
