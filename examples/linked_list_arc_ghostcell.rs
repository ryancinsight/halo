//! RustBelt / GhostCell paper style example: shared linked list with GhostCell.
//!
//! Here *permissions* (the token) are separated from *data* (the nodes).
//! Reads require `&GhostToken`, writes require `&mut GhostToken`, and no locks
//! are needed for aliasing control.

use std::sync::Arc;

use halo::{GhostCell, GhostToken};

#[derive(Default)]
struct Node<'brand> {
    value: i32,
    next: Option<Arc<GhostCell<'brand, Node<'brand>>>>,
}

fn main() {
    GhostToken::new(|mut token| {
        // Build a 3-node list: 1 -> 2 -> 3
        let n3: Arc<GhostCell<'_, Node<'_>>> = Arc::new(GhostCell::new(Node {
            value: 3,
            next: None,
        }));
        let n2: Arc<GhostCell<'_, Node<'_>>> = Arc::new(GhostCell::new(Node {
            value: 2,
            next: Some(Arc::clone(&n3)),
        }));
        let n1: Arc<GhostCell<'_, Node<'_>>> = Arc::new(GhostCell::new(Node {
            value: 1,
            next: Some(Arc::clone(&n2)),
        }));

        // Read traversal (no locks; token gates borrows).
        let mut sum = 0;
        let mut cur: Option<Arc<GhostCell<'_, Node<'_>>>> = Some(n1);
        while let Some(node) = cur {
            let r = node.borrow(&token);
            sum += r.value;
            cur = r.next.as_ref().map(Arc::clone);
        }
        assert_eq!(sum, 6);

        // Mutate traversal (exclusive token).
        let mut cur: Option<Arc<GhostCell<'_, Node<'_>>>> = Some(Arc::clone(&n2));
        while let Some(node) = cur {
            let w = node.borrow_mut(&mut token);
            w.value *= 10;
            cur = w.next.as_ref().map(Arc::clone);
        }

        assert_eq!(n2.borrow(&token).value, 20);
        assert_eq!(n3.borrow(&token).value, 30);
    });
}
