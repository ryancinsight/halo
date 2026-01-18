use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{GhostToken, BrandedVec, BrandedDoublyLinkedList, BrandedVecDeque};
use halo::collections::{BrandedBinaryHeap, BrandedDeque, BrandedChunkedVec};
use std::collections::{VecDeque, LinkedList, BinaryHeap};

fn bench_iterators(c: &mut Criterion) {
    let mut group = c.benchmark_group("iterators");

    const SIZE: usize = 1000;

    // BrandedVec iter
    group.bench_function("BrandedVec::iter", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            for i in 0..SIZE {
                vec.push(i);
            }
            b.iter(|| {
                let count = vec.iter(&token).count();
                black_box(count);
            });
        });
    });

    // Std Vec iter
    group.bench_function("Vec::iter", |b| {
        let mut vec = Vec::new();
        for i in 0..SIZE {
            vec.push(i);
        }
        b.iter(|| {
            let count = vec.iter().count();
            black_box(count);
        });
    });

    // BrandedDoublyLinkedList iter
    group.bench_function("BrandedDoublyLinkedList::iter", |b| {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            for i in 0..SIZE {
                list.push_back(&mut token, i);
            }
            b.iter(|| {
                let count = list.iter(&token).count();
                black_box(count);
            });
        });
    });

    // Std LinkedList iter
    group.bench_function("LinkedList::iter", |b| {
        let mut list = LinkedList::new();
        for i in 0..SIZE {
            list.push_back(i);
        }
        b.iter(|| {
            let count = list.iter().count();
            black_box(count);
        });
    });

    // BrandedDeque iter
    group.bench_function("BrandedDeque::iter", |b| {
        GhostToken::new(|mut token| {
            let mut deque: BrandedDeque<_, SIZE> = BrandedDeque::new();
            for i in 0..SIZE {
                deque.push_back(i).unwrap();
            }
            b.iter(|| {
                let count = deque.iter(&token).count();
                black_box(count);
            });
        });
    });

    // BrandedVecDeque iter
    group.bench_function("BrandedVecDeque::iter", |b| {
        GhostToken::new(|mut token| {
            let mut deque = BrandedVecDeque::new();
            for i in 0..SIZE {
                deque.push_back(i);
            }
            b.iter(|| {
                let count = deque.iter(&token).count();
                black_box(count);
            });
        });
    });

    // Std VecDeque iter
    group.bench_function("VecDeque::iter", |b| {
        let mut deque = VecDeque::new();
        for i in 0..SIZE {
            deque.push_back(i);
        }
        b.iter(|| {
            let count = deque.iter().count();
            black_box(count);
        });
    });

    // BrandedChunkedVec iter
    group.bench_function("BrandedChunkedVec::iter", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedChunkedVec::<_, 64>::new();
            for i in 0..SIZE {
                vec.push(i);
            }
            b.iter(|| {
                let count = vec.iter(&token).count();
                black_box(count);
            });
        });
    });

    // BrandedBinaryHeap iter
    group.bench_function("BrandedBinaryHeap::iter", |b| {
        GhostToken::new(|mut token| {
            let mut heap = BrandedBinaryHeap::new();
            for i in 0..SIZE {
                heap.push(&mut token, i);
            }
            b.iter(|| {
                let count = heap.iter(&token).count();
                black_box(count);
            });
        });
    });

    // Std BinaryHeap iter
    group.bench_function("BinaryHeap::iter", |b| {
        let mut heap = BinaryHeap::new();
        for i in 0..SIZE {
            heap.push(i);
        }
        b.iter(|| {
            let count = heap.iter().count();
            black_box(count);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_iterators);
criterion_main!(benches);
