use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedIntervalMap;
use halo::GhostToken;
use std::collections::BTreeMap;
use std::ops::Bound;

fn bench_interval_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("interval_map");

    // Insert 1000 items (disjoint)
    // BrandedIntervalMap maintains sorted contiguous memory (O(N^2) total for N insertions due to shifting)
    // vs BTreeMap (O(N log N)).
    group.bench_function("branded_interval_map_insert_1000", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut map = BrandedIntervalMap::new();
                for i in 0..1000 {
                    // [i*10, i*10 + 5)
                    map.insert(
                        &mut token,
                        black_box(i * 10),
                        black_box(i * 10 + 5),
                        black_box(i),
                    );
                }
            });
        });
    });

    group.bench_function("btreemap_insert_1000_naive", |b| {
        b.iter(|| {
            let mut map = BTreeMap::new();
            for i in 0..1000 {
                map.insert(black_box(i * 10), (black_box(i * 10 + 5), black_box(i)));
            }
        });
    });

    // Lookup 1000 items
    // BrandedIntervalMap uses binary search on a slice (very cache friendly)
    // BTreeMap uses tree traversal (pointer chasing)
    group.bench_function("branded_interval_map_get", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedIntervalMap::new();
            for i in 0..1000 {
                map.insert(&mut token, i * 10, i * 10 + 5, i);
            }

            b.iter(|| {
                for i in 0..1000 {
                    // Hit middle of interval
                    black_box(map.get(&token, black_box(i * 10 + 2)));
                }
            });
        });
    });

    group.bench_function("btreemap_get", |b| {
        let mut map = BTreeMap::new();
        for i in 0..1000 {
            map.insert(i * 10, (i * 10 + 5, i));
        }

        b.iter(|| {
            for i in 0..1000 {
                let point = black_box(i * 10 + 2);
                // Standard way to find interval in BTreeMap: find key <= point
                let entry = map
                    .range((Bound::Unbounded, Bound::Included(point)))
                    .next_back();

                if let Some((_, (end, val))) = entry {
                    if *end > point {
                        black_box(val);
                    }
                }
            }
        });
    });

    // Iteration
    // BrandedIntervalMap iterates contiguous memory
    // BTreeMap iterates nodes in heap
    group.bench_function("branded_interval_map_iter", |b| {
        GhostToken::new(|mut token| {
            let mut map = BrandedIntervalMap::new();
            for i in 0..1000 {
                map.insert(&mut token, i * 10, i * 10 + 5, i);
            }

            b.iter(|| {
                for entry in map.iter(&token) {
                    black_box(entry);
                }
            });
        });
    });

    group.bench_function("btreemap_iter", |b| {
        let mut map = BTreeMap::new();
        for i in 0..1000 {
            map.insert(i * 10, (i * 10 + 5, i));
        }

        b.iter(|| {
            for entry in map.iter() {
                black_box(entry);
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_interval_map);
criterion_main!(benches);
