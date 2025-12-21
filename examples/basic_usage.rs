//! Basic GhostCell usage example

use halo::{GhostCell, GhostToken, GhostUnsafeCell};

fn main() {
    println!("GhostCell Basic Usage Example");
    println!("=============================");

    GhostToken::new(|mut token| {
        // Create cells with different data types
        let int_cell = GhostCell::new(42);
        let vec_cell = GhostCell::new(vec![1, 2, 3, 4, 5]);

        // Immutable borrowing
        println!("Initial values:");
        println!("  Integer: {}", *int_cell.borrow(&token));
        println!("  Vector length: {}", vec_cell.borrow(&token).len());

        // Mutable borrowing
        *int_cell.borrow_mut(&mut token) = 100;
        vec_cell.borrow_mut(&mut token).push(6);

        println!("After mutation:");
        println!("  Integer: {}", *int_cell.borrow(&token));
        println!("  Vector length: {}", vec_cell.borrow(&token).len());

        // Efficient operations for Copy types
        let copy_value = int_cell.get(&token);
        int_cell.set(&mut token, copy_value * 2);
        println!("  Doubled: {}", int_cell.get(&token));
    });

    // Demonstrate the raw `GhostUnsafeCell` foundation
    println!("\nGhostUnsafeCell (raw foundation):");
    GhostToken::new(|mut token| {
        let cell = GhostUnsafeCell::new(vec![1, 2, 3]);

        let borrowed = cell.get(&token);
        println!("  Borrowed: {:?}", *borrowed);

        let borrowed_mut = cell.get_mut(&mut token);
        borrowed_mut.push(4);
        println!("  After push: {:?}", *borrowed_mut);
    });
}
