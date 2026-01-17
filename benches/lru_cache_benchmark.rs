use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedLruCache;
use halo::GhostToken;
use std::collections::HashMap;

fn bench_lru_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("lru_cache");

    // Baseline: HashMap insert (no LRU logic)
    group.bench_function("std_hash_map_insert", |b| {
        b.iter(|| {
            let mut map = HashMap::new();
            for i in 0..1000 {
                map.insert(black_box(i), black_box(i));
            }
        });
    });

    group.bench_function("branded_lru_cache_put", |b| {
        b.iter(|| {
            GhostToken::new(|mut token| {
                let mut cache = BrandedLruCache::new(1000);
                for i in 0..1000 {
                    cache.put(&mut token, black_box(i), black_box(i));
                }
            });
        });
    });

    // Baseline: HashMap get
    // Pre-create data
    let data: Vec<i32> = (0..1000).collect();

    group.bench_function("std_hash_map_get", |b| {
        let mut map = HashMap::new();
        for &i in &data {
            map.insert(i, i);
        }
        b.iter(|| {
            for i in 0..1000 {
                 black_box(map.get(&i));
            }
        });
    });

    group.bench_function("branded_lru_cache_get", |b| {
        b.iter(|| {
             GhostToken::new(|mut token| {
                let mut cache = BrandedLruCache::new(1000);
                for i in 0..1000 {
                    cache.put(&mut token, i, i);
                }
                // We measure only get loop?
                // But we are inside iter, so we re-create cache every time.
                // This includes setup cost.
                // To measure only get, we need to create cache outside.
                // But token binds to closure.
                // So we are forced to measure creation + get.
                // To minimize creation impact, we can do more gets.
                for _ in 0..10 {
                    for i in 0..1000 {
                        black_box(cache.get(&mut token, &i));
                    }
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_lru_cache);
criterion_main!(benches);
