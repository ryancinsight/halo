//! RustBelt / GhostCell paper style example: shared linked list with `Arc<RwLock<_>>`.
//!
//! This is the “conventional” Rust approach: permissions are coupled to the data via locks.
//! It is correct but imposes runtime synchronization overhead on every access.

use std::sync::{Arc, RwLock};

#[derive(Default)]
struct Node {
    value: i32,
    next: Option<Arc<RwLock<Node>>>,
}

fn main() {
    // Build a 3-node list: 1 -> 2 -> 3
    let n3 = Arc::new(RwLock::new(Node {
        value: 3,
        next: None,
    }));
    let n2 = Arc::new(RwLock::new(Node {
        value: 2,
        next: Some(Arc::clone(&n3)),
    }));
    let n1 = Arc::new(RwLock::new(Node {
        value: 1,
        next: Some(Arc::clone(&n2)),
    }));

    // Read traversal (locks on every hop).
    let mut sum = 0;
    let mut cur = Some(n1);
    while let Some(node) = cur {
        let g = node.read().unwrap();
        sum += g.value;
        cur = g.next.as_ref().map(Arc::clone);
    }
    assert_eq!(sum, 6);

    // Mutate traversal (write locks).
    let mut cur = Some(Arc::clone(&n2));
    while let Some(node) = cur {
        let mut g = node.write().unwrap();
        g.value *= 10;
        cur = g.next.as_ref().map(Arc::clone);
    }

    let v2 = n2.read().unwrap().value;
    let v3 = n3.read().unwrap().value;
    assert_eq!(v2, 20);
    assert_eq!(v3, 30);
}
