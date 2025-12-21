//! Basic tests for core GhostCell functionality

use halo::*;

#[test]
fn test_basic_ghostcell_operations() {
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(42);

        // Test immutable borrow
        assert_eq!(*cell.borrow(&token), 42);

        // Test mutable borrow
        *cell.borrow_mut(&mut token) = 100;
        assert_eq!(*cell.borrow(&token), 100);
    });
}

#[test]
fn test_zero_copy_operations() {
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(vec![1, 2, 3, 4, 5]);

        // Zero-copy borrowing
        let borrowed = cell.borrow(&token);
        assert_eq!(borrowed.len(), 5);
        assert_eq!(borrowed[0], 1);

        // Zero-copy mutation
        cell.borrow_mut(&mut token).push(6);
        assert_eq!(cell.borrow(&token).len(), 6);
    });
}

#[test]
fn test_copy_types_optimization() {
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(42i32);

        // Direct get/set for Copy types (efficient)
        assert_eq!(cell.get(&token), 42);
        cell.set(&mut token, 100);
        assert_eq!(cell.get(&token), 100);
    });
}

#[test]
fn test_phantom_type_safety() {
    // Test that different token brands cannot mix
    GhostToken::new(|token1| {
        let cell1 = GhostCell::new(1);

        GhostToken::new(|token2| {
            let cell2 = GhostCell::new(2);

            // These should work fine within their scopes
            assert_eq!(*cell1.borrow(&token1), 1);
            assert_eq!(*cell2.borrow(&token2), 2);

            // But we cannot mix tokens and cells from different scopes
            // This would be a compile error if uncommented:
            // assert_eq!(*cell1.borrow(&token2), 1); // Compile error!
        });
    });
}

#[test]
fn test_ghostcell_vec_borrowing() {
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(vec![1, 2, 3]);

        // Test borrowing
        {
            let borrowed = cell.borrow(&token);
            assert_eq!(*borrowed, vec![1, 2, 3]);
        }

        {
            let borrowed_mut = cell.borrow_mut(&mut token);
            borrowed_mut.push(4);
            assert_eq!(*borrowed_mut, vec![1, 2, 3, 4]);
        }
    });
}

#[test]
fn test_mathematical_correctness() {
    // Test commutative operations
    GhostToken::new(|token| {
        let a = GhostCell::new(5);
        let b = GhostCell::new(3);

        // Commutativity: a + b = b + a
        let sum1 = *a.borrow(&token) + *b.borrow(&token);
        let sum2 = *b.borrow(&token) + *a.borrow(&token);
        assert_eq!(sum1, sum2);

        // Test associative operations
        let c = GhostCell::new(2);
        let left_assoc = (*a.borrow(&token) + *b.borrow(&token)) + *c.borrow(&token);
        let right_assoc = *a.borrow(&token) + (*b.borrow(&token) + *c.borrow(&token));
        assert_eq!(left_assoc, right_assoc);
    });
}
