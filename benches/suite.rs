use criterion::{criterion_group, criterion_main, Criterion};

mod workloads;

#[cfg(feature = "alloc-halo")]
use halo::allocator::HaloAllocator;

#[cfg(feature = "alloc-halo")]
#[global_allocator]
static GLOBAL: HaloAllocator = HaloAllocator;

#[cfg(feature = "alloc-mimalloc")]
use mimalloc::MiMalloc;

#[cfg(feature = "alloc-mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(feature = "alloc-snmalloc")]
use snmalloc_rs::SnMalloc;

#[cfg(feature = "alloc-snmalloc")]
#[global_allocator]
static GLOBAL: SnMalloc = SnMalloc;

#[cfg(feature = "alloc-jemalloc")]
use jemallocator::Jemalloc;

#[cfg(feature = "alloc-jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn bench_main(c: &mut Criterion) {
    workloads::micro::run(c);
    workloads::larson::run(c);
    workloads::threadtest::run(c);
    workloads::shbench::run(c);
}

criterion_group!(benches, bench_main);
criterion_main!(benches);
