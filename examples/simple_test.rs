#[cfg(feature = "alloc-halo")]
use halo::allocator::HaloAllocator;

#[cfg(feature = "alloc-halo")]
#[global_allocator]
static GLOBAL: HaloAllocator = HaloAllocator;

fn main() {
    println!("Start");
    let b = Box::new(42);
    println!("Box: {}", b);
    let v = vec![0u8; 100];
    println!("Vec len: {}", v.len());
    println!("Done");
}
