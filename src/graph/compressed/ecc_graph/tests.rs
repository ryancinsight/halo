//! Tests for ECC graph implementation.

use super::*;
use crate::GhostToken;

#[test]
fn ecc_graph_basic_operations() {
    GhostToken::new(|_token| {
        let adjacency = vec![
            vec![1, 2, 3],
            vec![0, 2],
            vec![0, 1, 3],
            vec![0, 2],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 10);
        assert_eq!(graph.degree(0), 3);
        assert_eq!(graph.degree(1), 2);

        // Test neighbors
        let neighbors_0: Vec<_> = graph.neighbors(0).collect();
        assert_eq!(neighbors_0.len(), 3);
        assert!(neighbors_0.contains(&1));
        assert!(neighbors_0.contains(&2));
        assert!(neighbors_0.contains(&3));
    });
}

#[test]
fn ecc_graph_triangle_counting() {
    GhostToken::new(|_token| {
        // Triangle: 0-1-2-0
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
            vec![], // Isolated node
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        assert_eq!(graph.triangle_count(), 1);

        // No triangles
        let empty_graph = GhostEccGraph::from_adjacency(&[vec![], vec![]]);
        assert_eq!(empty_graph.triangle_count(), 0);
    });
}

#[test]
fn ecc_graph_clustering_coefficient() {
    GhostToken::new(|_token| {
        // Complete graph K3
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);

        // In K3, clustering coefficient should be 1.0
        for node in 0..3 {
            assert!((graph.clustering_coefficient(node) - 1.0).abs() < 1e-6);
        }

        assert!((graph.average_clustering_coefficient() - 1.0).abs() < 1e-6);
    });
}

#[test]
fn ecc_graph_stats() {
    GhostToken::new(|_token| {
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2],
            vec![0, 1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        let stats = graph.graph_stats();

        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 6); // Complete graph
        assert_eq!(stats.triangles, 1); // K3 has 1 triangle
        assert!(stats.memory_usage > 0);
        assert!(stats.average_clustering >= 0.0 && stats.average_clustering <= 1.0);
    });
}

#[test]
fn ecc_graph_bfs() {
    GhostToken::new(|_token| {
        let adjacency = vec![
            vec![1, 2],
            vec![0, 2, 3],
            vec![0, 1],
            vec![1],
        ];

        let graph = GhostEccGraph::from_adjacency(&adjacency);
        let traversal = graph.bfs(0);

        assert!(!traversal.is_empty());
        assert_eq!(traversal[0], 0);
        assert!(traversal.contains(&1));
        assert!(traversal.contains(&2));
        assert!(traversal.contains(&3));
    });
}

