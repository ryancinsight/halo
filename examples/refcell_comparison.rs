//! Performance demonstration: `GhostCell` vs `std::cell::RefCell`.

use halo::{GhostCell, GhostToken};
use std::{cell::RefCell, time::Instant};

fn main() {
    println!("GhostCell vs RefCell Demonstration");
    println!("==================================");

    let iterations = 1_000_000;
    println!("Iterations: {iterations}\n");

    println!("Timings (read+write loop):");
    bench::<u64>(iterations, 42);
    bench::<Vec<i32>>(iterations, vec![1, 2, 3, 4, 5]);
    bench::<String>(iterations, "Hello World".to_string());

    println!("\nMemory Footprint (type size only):");
    memory::<u8>();
    memory::<u64>();
    memory::<Vec<i32>>();
    memory::<String>();
}

fn bench<T>(iterations: usize, initial: T)
where
    T: Clone,
{
    let ghost_time = GhostToken::new(|mut token| {
        let cell = GhostCell::new(initial.clone());
        let start = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(cell.borrow(&token));
            std::hint::black_box(cell.borrow_mut(&mut token));
        }
        start.elapsed()
    });

    let refcell_time = {
        let cell = RefCell::new(initial);
        let start = Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(cell.borrow());
            std::hint::black_box(cell.borrow_mut());
        }
        start.elapsed()
    };

    println!(
        "{}: ghost={:?}, refcell={:?}",
        std::any::type_name::<T>(),
        ghost_time,
        refcell_time
    );
}

fn memory<T>() {
    let ghost = std::mem::size_of::<GhostCell<'static, T>>();
    let refcell = std::mem::size_of::<RefCell<T>>();
    println!(
        "{}: ghostcell={} bytes, refcell={} bytes",
        std::any::type_name::<T>(),
        ghost,
        refcell
    );
}
