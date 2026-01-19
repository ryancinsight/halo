use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::TripodList;
use halo::GhostToken;
use std::collections::LinkedList;

fn bench_tripod_list_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("tripod_list_iter");

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

    group.bench_function("tripod_list_iter", |b| {
        GhostToken::new(|mut token| {
            let mut list = TripodList::new();
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

fn bench_tripod_list_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("tripod_list_push_pop");

    group.bench_function("std_linked_list_push_pop", |b| {
        b.iter(|| {
            let mut list = LinkedList::new();
            for i in 0..1000 {
                list.push_back(i);
            }
            while let Some(_) = list.pop_front() {}
        });
    });

    group.bench_function("tripod_list_push_pop", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut list = TripodList::new();
                for i in 0..1000 {
                    list.push_back(&mut token, i);
                }
                while let Some(_) = list.pop_front(&mut token) {}
            });
        });
    });

    group.finish();
}

fn bench_tripod_list_parent(c: &mut Criterion) {
    let mut group = c.benchmark_group("tripod_list_parent");

    group.bench_function("tripod_list_parent_access", |b| {
        // Setup
        GhostToken::new(|mut token| {
            let mut list = TripodList::new();
            list.set_default_parent(Some(123));
            let mut indices = Vec::with_capacity(1000);
            for i in 0..1000 {
                indices.push(list.push_back(&mut token, i));
            }

            b.iter(|| {
                let mut sum_parents = 0;
                for &idx in &indices {
                    if let Some(p) = list.get_parent(&token, idx) {
                        sum_parents += p;
                    }
                }
                black_box(sum_parents);
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_tripod_list_iter,
    bench_tripod_list_push_pop,
    bench_tripod_list_parent
);
criterion_main!(benches);
