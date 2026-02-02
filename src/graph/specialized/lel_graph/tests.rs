use super::*;
use crate::GhostToken;

#[test]
fn lel_graph_basic_operations() {
    let adjacency = vec![vec![1, 2, 3], vec![0, 2], vec![0, 1, 3], vec![0, 2]];

    GhostToken::new(|token| {
        let graph = GhostLelGraph::from_adjacency(&adjacency);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 10);
        assert_eq!(graph.degree(&token, 0), 3);
        assert_eq!(graph.degree(&token, 1), 2);

        let neighbors_0: Vec<_> = graph.neighbors(0).collect();
        assert_eq!(neighbors_0.len(), 3);
        assert!(neighbors_0.contains(&1));
        assert!(neighbors_0.contains(&2));
        assert!(neighbors_0.contains(&3));
    });
}

#[test]
fn delta_encoded_edges() {
    let adjacency = vec![vec![1, 2], vec![2], vec![]];
    GhostToken::new(|token| {
        let graph = GhostLelGraph::from_adjacency(&adjacency);
        assert_eq!(graph.degree(&token, 0), 2);
        assert_eq!(graph.degree(&token, 1), 1);
        assert_eq!(graph.degree(&token, 2), 0);
    });
}

#[test]
fn lel_compression_stats() {
    let adjacency = vec![vec![1, 2], vec![2], vec![]];
    GhostToken::new(|token| {
        let graph = GhostLelGraph::from_adjacency(&adjacency);
        let stats = graph.compression_stats(&token);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 3);
        assert!(stats.compressed_size > 0);
    });
}

#[test]
fn lel_graph_bfs() {
    let adjacency = vec![vec![1, 2], vec![0, 2, 3], vec![0, 1], vec![1]];
    let graph = GhostLelGraph::from_adjacency(&adjacency);
    let traversal = graph.bfs(0);
    assert!(!traversal.is_empty());
    assert_eq!(traversal[0], 0);
    assert!(traversal.contains(&1));
    assert!(traversal.contains(&2));
    assert!(traversal.contains(&3));
}
