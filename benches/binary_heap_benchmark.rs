use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedBinaryHeap;
use halo::GhostToken;
use std::collections::BinaryHeap;

fn bench_binary_heap_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_heap_push_pop");
    let size = 1000;

    group.bench_function("std_binary_heap_push_pop", |b| {
        b.iter(|| {
            let mut heap = BinaryHeap::with_capacity(size);
            for i in 0..size {
                heap.push(black_box(i));
            }
            for _ in 0..size {
                black_box(heap.pop());
            }
        });
    });

    group.bench_function("branded_binary_heap_push_pop", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut heap = BrandedBinaryHeap::with_capacity(size);
                for i in 0..size {
                    heap.push(&token, black_box(i));
                }
                for _ in 0..size {
                    black_box(heap.pop(&token));
                }
            });
        });
    });

    group.finish();
}

fn bench_binary_heap_peek(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_heap_peek");
    let size = 1000;

    // Setup heaps
    let std_heap: BinaryHeap<usize> = (0..size).collect();

    group.bench_function("std_binary_heap_peek", |b| {
        b.iter(|| {
            black_box(std_heap.peek());
        });
    });

    group.bench_function("branded_binary_heap_peek", |b| {
        GhostToken::new(|token| {
            let mut heap = BrandedBinaryHeap::with_capacity(size);
            for i in 0..size {
                heap.push(&token, i);
            }

            b.iter(|| {
                black_box(heap.peek(&token));
            });
        });
    });

    group.finish();
}

fn bench_binary_heap_peek_mut(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_heap_peek_mut");

    group.bench_function("std_binary_heap_peek_mut_modify", |b| {
        b.iter(|| {
            // We need a fresh heap or at least a valid one for each iter if we modify it significantly
            // But usually peek_mut is fast. Let's just create a small one.
            let mut heap: BinaryHeap<i32> = BinaryHeap::from(vec![1, 5, 10]);
            if let Some(mut top) = heap.peek_mut() {
                *top = black_box(2);
            }
            // Heap is re-sifted here
            black_box(heap.pop());
        });
    });

    group.bench_function("branded_binary_heap_peek_mut_modify", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut heap = BrandedBinaryHeap::new();
                heap.push(&token, 1);
                heap.push(&token, 5);
                heap.push(&token, 10);

                if let Some(mut top) = heap.peek_mut(&mut token) {
                    *top = black_box(2);
                }
                // Heap is re-sifted here
                black_box(heap.pop(&token));
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_binary_heap_push_pop,
    bench_binary_heap_peek,
    bench_binary_heap_peek_mut
);
criterion_main!(benches);
