use super::*;

#[test]
fn amt_graph_basic_operations() {
    let mut graph = GhostAmtGraph::<64>::new(10);

    // Add some edges
    graph.add_edge(0, 1);
    graph.add_edge(0, 2);
    graph.add_edge(1, 2);

    assert_eq!(graph.node_count(), 10);
    assert_eq!(graph.edge_count(), 3);
    assert_eq!(graph.degree(0), 2);
    assert_eq!(graph.degree(1), 1);
    assert_eq!(graph.degree(2), 0);

    assert!(graph.has_edge(0, 1));
    assert!(graph.has_edge(0, 2));
    assert!(graph.has_edge(1, 2));
    assert!(!graph.has_edge(2, 0));

    // Check neighbors
    let neighbors_0: Vec<_> = graph.neighbors(0).collect();
    assert_eq!(neighbors_0.len(), 2);
    assert!(neighbors_0.contains(&1));
    assert!(neighbors_0.contains(&2));
}

#[test]
fn amt_graph_representation_upgrade() {
    let mut graph = GhostAmtGraph::<64>::new(100);

    // Add many edges to trigger representation upgrades
    let node = 0;
    for i in 1..50 {
        graph.add_edge(node, i);
    }

    // Should upgrade to sorted representation
    match &graph.nodes[node] {
        representation::NodeRepresentation::Sorted { .. } => {}
        _ => panic!("Expected sorted representation"),
    }

    assert_eq!(graph.degree(node), 49);
    assert!(graph.has_edge(node, 25));
}



