use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedBinaryHeap;
use halo::GhostToken;
use std::collections::BinaryHeap;

fn bench_binary_heap(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_heap");

    group.bench_function("std_binary_heap_push", |b| {
        b.iter(|| {
            let mut heap = BinaryHeap::new();
            for i in 0..1000 {
                heap.push(black_box(i));
            }
        });
    });

    group.bench_function("branded_binary_heap_push", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut heap = BrandedBinaryHeap::new();
                for i in 0..1000 {
                    heap.push(&mut token, black_box(i));
                }
            });
        });
    });

    // Combined push and pop because we can't easily isolate pop with branded types
    // without including setup cost or GhostToken creation in the loop.
    group.bench_function("std_binary_heap_push_pop", |b| {
        b.iter(|| {
            let mut heap = BinaryHeap::new();
            for i in 0..1000 {
                heap.push(i);
            }
            while let Some(x) = heap.pop() {
                black_box(x);
            }
        });
    });

    group.bench_function("branded_binary_heap_push_pop", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut heap = BrandedBinaryHeap::new();
                for i in 0..1000 {
                    heap.push(&mut token, i);
                }
                while let Some(x) = heap.pop(&mut token) {
                    black_box(x);
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_binary_heap);
criterion_main!(benches);
