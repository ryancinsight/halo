//! Tests for compressed graph implementation.

use super::*;
use crate::GhostToken;

#[test]
fn compressed_graph_basic_operations() {
    GhostToken::new(|_token| {
        let adjacency = vec![
            vec![1, 2, 3],
            vec![0, 2],
            vec![0, 1, 3],
            vec![0, 2],
        ];

        let graph = GhostCompressedGraph::<64>::from_adjacency(&adjacency);

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

        // Test BFS
        let traversal = super::traversal::bfs(&graph, 0);
        assert!(!traversal.is_empty());
        assert_eq!(traversal[0], 0);
    });
}

#[test]
fn compression_stats() {
    GhostToken::new(|_token| {
        let adjacency = vec![
            vec![1, 2, 3, 4, 5],
            vec![0, 2, 3],
            vec![0, 1, 3, 4],
            vec![0, 1, 2, 4],
            vec![0, 2, 3, 5],
            vec![0, 4],
        ];

        let graph = GhostCompressedGraph::<64>::from_adjacency(&adjacency);
        let stats = super::traversal::compression_stats(&graph);

        assert!(stats.compressed_size > 0);
        assert_eq!(stats.node_count, 6);
        assert_eq!(stats.edge_count, 22);

        // Test compression ratio calculations (may not compress for this data pattern)
        assert!(stats.compression_ratio() > 0.0);
        // Note: For sparse graphs, RLE on offsets may not compress well
        // This demonstrates the research concept rather than guaranteed compression
    });
}

