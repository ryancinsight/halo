use halo::allocator::HaloAllocator;
use std::time::Instant;

#[global_allocator]
static ALLOC: HaloAllocator = HaloAllocator;

fn main() {
    let start = Instant::now();
    const ITER: usize = 1000;
    for _ in 0..ITER {
        let mut v = Vec::with_capacity(1000);
        for i in 0..1000 {
            v.push(Box::new(i));
        }
    }
    println!("Elapsed for {} iters: {:?}", ITER, start.elapsed());
}
