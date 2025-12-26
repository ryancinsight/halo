//! BrandedHashMap usage example.
//!
//! Demonstrates how branding protects values in a hash map, allowing
//! shared access to the map while still controlling access to values.

use halo::{BrandedHashMap, GhostToken};

fn main() {
    println!("BrandedHashMap Usage Example");
    println!("============================");

    GhostToken::new(|mut token| {
        let mut map = BrandedHashMap::new();
        map.insert("alice", 100);
        map.insert("bob", 200);
        map.insert("charlie", 300);

        println!("Initial map length: {}", map.len());

        // Shared read-only access to map and token.
        println!("\nReading values (shared):");
        if let Some(val) = map.get(&token, &"alice") {
            println!("  alice: {}", val);
        }

        // Exclusive access to values via &mut token.
        println!("\nMutating values (exclusive):");
        if let Some(val) = map.get_mut(&mut token, &"bob") {
            *val += 50;
            println!("  bob updated to: {}", val);
        }

        // Iterate over keys (no token needed).
        println!("\nKeys in map:");
        for key in map.keys() {
            println!("  - {}", key);
        }

        // Iterate over values (token needed).
        println!("\nValues in map (via token):");
        for val in map.values(&token) {
            println!("  - {}", val);
        }

        // Remove an entry.
        println!("\nRemoving 'charlie'...");
        let old_val = map.remove(&"charlie");
        println!("  Removed value: {:?}", old_val);
        println!("Final map length: {}", map.len());
    });
}

