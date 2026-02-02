use halo::alloc::HaloAllocator;

#[global_allocator]
static ALLOC: HaloAllocator = HaloAllocator;

fn main() {
    println!("Start");
    let mut v = Vec::new();
    for i in 0..100000 {
        v.push(i);
    }
    println!("Allocated vec with {} elements", v.len());
}
