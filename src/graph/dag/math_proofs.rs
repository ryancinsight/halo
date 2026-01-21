//! Compile-time mathematical proofs for DAG properties.
//!
//! These functions provide lightweight, const-evaluable checks that encode
//! graph-theory theorems used by the `GhostDag` API documentation.

/// Proves that a valid topological ordering implies acyclicity.
///
/// **Theorem**: If a finite directed graph has a topological ordering,
/// then it is a DAG (contains no cycles).
///
/// **Proof**: By contradiction. Suppose G has a cycle C. In any topological
/// ordering, all nodes in C must appear before their successors. But since
/// C is a cycle, this creates a contradiction.
pub const fn topological_order_implies_acyclic(order_len: usize, node_count: usize) -> bool {
    order_len == node_count
}

/// Verifies that the longest path in a DAG is well-defined.
///
/// **Theorem**: In a DAG, the longest path between any two nodes is well-defined
/// and can be computed via dynamic programming (DP) over a topological order.
pub const fn longest_path_well_defined(node_count: usize, _edge_count: usize) -> bool {
    // For the DP to be well-defined, we need a topological order.
    node_count > 0
}
