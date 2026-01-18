use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedDoublyLinkedList;
use halo::GhostToken;
use std::collections::LinkedList;

fn bench_linked_list_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("linked_list_iter");

    group.bench_function("std_linked_list_iter", |b| {
        let mut list = LinkedList::new();
        for i in 0..1000 {
            list.push_back(i);
        }
        b.iter(|| {
            let mut sum = 0;
            for x in &list {
                sum += *x;
            }
            black_box(sum);
        });
    });

    group.bench_function("branded_linked_list_iter", |b| {
        GhostToken::new(|mut token| {
            let mut list = BrandedDoublyLinkedList::new();
            for i in 0..1000 {
                list.push_back(&mut token, i);
            }
            b.iter(|| {
                let mut sum = 0;
                for x in list.iter(&token) {
                    sum += *x;
                }
                black_box(sum);
            });
        });
    });

    group.finish();
}

fn bench_linked_list_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("linked_list_push_pop");

    group.bench_function("std_linked_list_push_pop", |b| {
        b.iter(|| {
            let mut list = LinkedList::new();
            for i in 0..1000 {
                list.push_back(i);
            }
            while let Some(_) = list.pop_front() {}
        });
    });

    group.bench_function("branded_linked_list_push_pop", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut list = BrandedDoublyLinkedList::new();
                for i in 0..1000 {
                    list.push_back(&mut token, i);
                }
                while let Some(_) = list.pop_front(&mut token) {}
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_linked_list_iter, bench_linked_list_push_pop);
criterion_main!(benches);
