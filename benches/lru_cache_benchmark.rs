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

    // String benchmarks
    group.bench_function("branded_lru_cache_put_string_large", |b| {
        // Pre-generate 1000 keys of length ~1000
        let keys: Vec<String> = (0..1000).map(|i| format!("{}-{}", "x".repeat(1000), i)).collect();
        b.iter(|| {
            // Clone keys to simulate fresh input
            let my_keys = keys.clone();
            GhostToken::new(|mut token| {
                let mut cache = BrandedLruCache::new(1000);
                for (i, s) in my_keys.into_iter().enumerate() {
                    cache.put(&mut token, s, i);
                }
            });
        });
    });

    group.bench_function("branded_lru_cache_get_string_large", |b| {
        let keys: Vec<String> = (0..1000).map(|i| format!("{}-{}", "x".repeat(1000), i)).collect();
        b.iter(|| {
             GhostToken::new(|mut token| {
                let mut cache = BrandedLruCache::new(1000);
                for (i, s) in keys.iter().enumerate() {
                    cache.put(&mut token, s.clone(), i);
                }

                for _ in 0..10 {
                    for k in &keys {
                        black_box(cache.get(&mut token, k));
                    }
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_lru_cache);
criterion_main!(benches);
