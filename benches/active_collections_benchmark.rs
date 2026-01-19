use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{GhostToken, BrandedHashSet};
use halo::collections::hash::active::ActivateHashSet;
use halo::collections::btree::BrandedBTreeSet;
use halo::collections::other::{BrandedDoublyLinkedList, BrandedBinaryHeap};
use halo::collections::other::active::{ActivateDoublyLinkedList, ActivateBinaryHeap};
use std::collections::{HashSet, BTreeSet, LinkedList, BinaryHeap};

fn bench_set_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("Set Insert");
    let size = 1000;

    group.bench_function("std::HashSet", |b| {
        b.iter(|| {
            let mut set = HashSet::new();
            for i in 0..size {
                set.insert(black_box(i));
            }
        })
    });

    group.bench_function("ActiveHashSet", |b| {
        b.iter(|| {
             GhostToken::new(|mut token| {
                let mut set = BrandedHashSet::new();
                {
                    let mut active = set.activate(&mut token);
                    for i in 0..size {
                        active.insert(black_box(i));
                    }
                }
            })
        })
    });

    group.finish();
}

fn bench_btree_set_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("BTreeSet Iter");
    let size = 1000;

    let mut std_set = BTreeSet::new();
    for i in 0..size {
        std_set.insert(i);
    }

    group.bench_function("std::BTreeSet", |b| {
        b.iter(|| {
            for x in std_set.iter() {
                black_box(x);
            }
        })
    });

    GhostToken::new(|_token| {
        let mut branded_set = BrandedBTreeSet::new();
        for i in 0..size {
            branded_set.insert(i);
        }

        group.bench_function("BrandedBTreeSet (Token-Free)", |b| {
            b.iter(|| {
                // Now supports iteration without token!
                for x in branded_set.iter() {
                    black_box(x);
                }
            })
        });
    });

    group.finish();
}

fn bench_linked_list_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("LinkedList Push");
    let size = 1000;

    group.bench_function("std::LinkedList", |b| {
        b.iter(|| {
            let mut list = LinkedList::new();
            for i in 0..size {
                list.push_back(black_box(i));
            }
        })
    });

    group.bench_function("ActiveDoublyLinkedList", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut list = BrandedDoublyLinkedList::new();
                {
                    let mut active = list.activate(&mut token);
                    for i in 0..size {
                        active.push_back(black_box(i));
                    }
                }
            })
        })
    });

    group.finish();
}

fn bench_binary_heap_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("BinaryHeap Push");
    let size = 1000;

    group.bench_function("std::BinaryHeap", |b| {
        b.iter(|| {
            let mut heap = BinaryHeap::new();
            for i in 0..size {
                heap.push(black_box(i));
            }
        })
    });

    group.bench_function("ActiveBinaryHeap", |b| {
        b.iter(|| {
             GhostToken::new(|mut token| {
                let mut heap = BrandedBinaryHeap::new();
                {
                    let mut active = heap.activate(&mut token);
                    for i in 0..size {
                        active.push(black_box(i));
                    }
                }
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_set_insert, bench_btree_set_iter, bench_linked_list_push, bench_binary_heap_push);
criterion_main!(benches);
