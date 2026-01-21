//! Tests for CSR graph implementation.

use super::*;

#[test]
fn test_csr_in_neighbors_basic() {
    // 0 -> 1
    // 1 -> 2
    // 2 -> 0
    let adjacency = vec![vec![1], vec![2], vec![0]];
    let graph = GhostCsrGraph::<4>::from_adjacency(&adjacency);

    assert_eq!(graph.in_neighbors(0), vec![2]);
    assert_eq!(graph.in_neighbors(1), vec![0]);
    assert_eq!(graph.in_neighbors(2), vec![1]);

    assert_eq!(graph.in_degree(0), 1);
    assert_eq!(graph.in_degree(1), 1);
    assert_eq!(graph.in_degree(2), 1);
}

#[test]
fn test_csr_in_neighbors_complex() {
    // 0 -> 1, 2
    // 1 -> 2
    // 2 ->
    // 3 -> 1
    let adjacency = vec![
        vec![1, 2],
        vec![2],
        vec![],
        vec![1],
    ];
    let graph = GhostCsrGraph::<4>::from_adjacency(&adjacency);

    // In-neighbors of 0: []
    assert!(graph.in_neighbors(0).is_empty());
    assert_eq!(graph.in_degree(0), 0);

    // In-neighbors of 1: [0, 3] (sorted order depends on construction, but likely 0 then 3)
    let mut in1 = graph.in_neighbors(1);
    in1.sort();
    assert_eq!(in1, vec![0, 3]);
    assert_eq!(graph.in_degree(1), 2);

    // In-neighbors of 2: [0, 1]
    let mut in2 = graph.in_neighbors(2);
    in2.sort();
    assert_eq!(in2, vec![0, 1]);
    assert_eq!(graph.in_degree(2), 2);

    // In-neighbors of 3: []
    assert!(graph.in_neighbors(3).is_empty());
    assert_eq!(graph.in_degree(3), 0);
}

#[test]
fn test_from_csr_parts_reconstruction() {
    // 0 -> 1, 2
    // 1 -> 2
    // 2 ->
    let offsets = vec![0, 2, 3, 3];
    let edges = vec![1, 2, 2];

    let graph = GhostCsrGraph::<4>::from_csr_parts(offsets, edges);

    // Check forward
    let mut n0: Vec<_> = graph.neighbors(0).collect();
    n0.sort();
    assert_eq!(n0, vec![1, 2]);

    // Check backward
    let mut in2 = graph.in_neighbors(2);
    in2.sort();
    assert_eq!(in2, vec![0, 1]);
    assert_eq!(graph.in_degree(2), 2);
}

#[test]
fn test_empty_graph() {
    let adjacency: Vec<Vec<usize>> = vec![];
    let graph = GhostCsrGraph::<4>::from_adjacency(&adjacency);
    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_disconnected_graph() {
    let adjacency = vec![vec![]; 5];
    let graph = GhostCsrGraph::<4>::from_adjacency(&adjacency);

    for i in 0..5 {
        assert_eq!(graph.in_degree(i), 0);
        assert!(graph.in_neighbors(i).is_empty());
    }
}
