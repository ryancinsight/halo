//! GhostLazyCell Usage Examples
//!
//! Demonstrates memory-efficient lazy initialization with GhostCell.

use halo::{GhostLazyCell, GhostToken};

fn main() {
    println!("GhostLazyCell Usage Examples");
    println!("============================");

    GhostToken::new(|mut token| {
        // Example 1: Basic lazy computation
        println!("\n1. Basic Lazy Computation:");
        let compute_count = std::cell::Cell::new(0);

        let lazy_value = GhostLazyCell::new(|| {
            compute_count.set(compute_count.get() + 1);
            println!("  Computing expensive value...");
            42 * 2
        });

        println!("  First access (triggers computation):");
        let value = lazy_value.get(&mut token);
        println!(
            "  Result: {}, Compute count: {}",
            *value,
            compute_count.get()
        );

        println!("  Second access (uses cache):");
        let value2 = lazy_value.get(&mut token);
        println!(
            "  Result: {}, Compute count: {}",
            *value2,
            compute_count.get()
        );

        // Example 2: GhostLazyCell caching behavior
        println!("\n2. GhostLazyCell Caching:");
        let compute_count2 = std::cell::Cell::new(0);

        let lazy = GhostLazyCell::new(|| {
            compute_count2.set(compute_count2.get() + 1);
            println!("  Computing expensive result #{}...", compute_count2.get());
            vec![1, 2, 3, 4, 5]
        });

        println!("  First access (computation):");
        let vec1 = lazy.get(&mut token);
        println!(
            "  Length: {}, Compute count: {}",
            vec1.len(),
            compute_count2.get()
        );

        println!("  Second access (cached):");
        let vec2 = lazy.get(&mut token);
        println!(
            "  Length: {}, Compute count: {}",
            vec2.len(),
            compute_count2.get()
        );

        // Example 3: Memory efficiency
        println!("\n3. Memory Efficiency:");
        let lazy_empty: GhostLazyCell<Vec<i32>> = GhostLazyCell::new(Vec::<i32>::new);

        println!(
            "  GhostLazyCell size: {} bytes",
            std::mem::size_of_val(&lazy_empty)
        );
        println!("  (Inline storage, no std::cell wrappers)");

        // After computation
        let _ = lazy_empty.get(&mut token);

        println!(
            "  GhostLazyCell size (computed): {} bytes",
            std::mem::size_of_val(&lazy_empty)
        );

        // Example 4: Performance demonstration
        println!("\n4. Performance Characteristics:");
        let start = std::time::Instant::now();
        let perf_lazy = GhostLazyCell::new(|| {
            // Simulate expensive computation
            // Use wrapping arithmetic so this example is correct in debug builds too.
            let mut sum: u64 = 0;
            for i in 0..100_000u64 {
                sum = sum.wrapping_add(i);
            }
            sum
        });

        // First access (computation)
        let first_access = start.elapsed();
        let _ = perf_lazy.get(&mut token);
        let computation_time = start.elapsed() - first_access;

        // Cached access
        let cached_start = std::time::Instant::now();
        let _ = perf_lazy.get(&mut token);
        let cached_time = cached_start.elapsed();

        println!("  Computation time: {:?}", computation_time);
        println!("  Cached access time: {:?}", cached_time);
        let cached_ns = cached_time.as_nanos();
        if cached_ns == 0 {
            println!(
                "  Speedup: > {:.0}x (cached measured as 0ns)",
                computation_time.as_nanos() as f64
            );
        } else {
            println!(
                "  Speedup: {:.0}x",
                computation_time.as_nanos() as f64 / cached_ns as f64
            );
        }
    });

    println!("\nKey Benefits:");
    println!("• Minimal memory overhead (stores only computation function initially)");
    println!("• Zero-cost cached access after computation");
    println!("• GhostOnceCell supports one-time initialization with token safety");
    println!("• Memory-efficient: defers allocation until needed");
}
