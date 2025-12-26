use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::{BrandedChunkedVec, BrandedVec, BrandedVecDeque, BrandedHashMap, BrandedHashSet, BrandedArena};
use halo::GhostToken;

fn bench_branded_chunked_vec_32(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_32", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 32> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_64(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_64", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 64> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_128(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_128", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 128> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_256(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_256", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 256> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_512(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_512", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 512> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_chunked_vec_1024(c: &mut Criterion) {
    c.bench_function("branded_chunked_vec_chunk_size_1024", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut vec: BrandedChunkedVec<'_, usize, 1024> = BrandedChunkedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                }
                vec.for_each(&token, |&x| {
                    black_box(x);
                });
            });
        });
    });
}

fn bench_branded_vec_operations(c: &mut Criterion) {
    c.bench_function("branded_vec_push_pop", |b| {
        b.iter(|| {
            let mut vec = BrandedVec::new();
            for i in 0..1000 {
                vec.push(i);
            }
            while let Some(_) = vec.pop() {}
        });
    });

    c.bench_function("branded_vec_access", |b| {
        GhostToken::new(|token| {
            let mut vec = BrandedVec::new();
            for i in 0..1000 {
                vec.push(i);
            }
            b.iter(|| {
                for i in 0..1000 {
                    black_box(vec.get(&token, i));
                }
            });
        });
    });

    c.bench_function("branded_vec_mutation", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            for i in 0..1000 {
                vec.push(i);
            }
            b.iter(|| {
                vec.for_each_mut(&mut token, |x| *x += 1);
            });
        });
    });
}

fn bench_branded_vec_deque_operations(c: &mut Criterion) {
    c.bench_function("branded_vec_deque_push_pop", |b| {
        b.iter(|| {
            let mut deque = BrandedVecDeque::new();
            for i in 0..500 {
                deque.push_back(i);
                deque.push_front(i);
            }
            for _ in 0..500 {
                deque.pop_front();
                deque.pop_back();
            }
        });
    });

    c.bench_function("branded_vec_deque_access", |b| {
        GhostToken::new(|token| {
            let mut deque = BrandedVecDeque::new();
            for i in 0..1000 {
                deque.push_back(i);
            }
            b.iter(|| {
                for i in 0..1000 {
                    black_box(deque.get(&token, i));
                }
            });
        });
    });
}

fn bench_branded_hash_map_operations(c: &mut Criterion) {
    c.bench_function("branded_hash_map_insert_lookup", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut map = BrandedHashMap::new();
                for i in 0..500 {
                    map.insert(i, i * 2);
                }
                for i in 0..500 {
                    black_box(map.get(&token, &i));
                }
            });
        });
    });

    c.bench_function("branded_hash_map_mixed_operations", |b| {
        GhostToken::new(|token| {
            let mut map = BrandedHashMap::new();
            b.iter(|| {
                // Insert phase
                for i in 0..200 {
                    map.insert(i, i);
                }
                // Lookup phase
                for i in 0..200 {
                    black_box(map.get(&token, &i));
                }
                // Remove phase
                for i in 100..200 {
                    map.remove(&i);
                }
            });
        });
    });
}

fn bench_branded_hash_set_operations(c: &mut Criterion) {
    c.bench_function("branded_hash_set_operations", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                let mut set = BrandedHashSet::new();
                for i in 0..500 {
                    set.insert(i);
                }
                for i in 0..500 {
                    black_box(set.contains_gated(&token, &i));
                }
                for i in 250..500 {
                    set.remove(&i);
                }
            });
        });
    });
}

fn bench_branded_arena_operations(c: &mut Criterion) {
    c.bench_function("branded_arena_allocation", |b| {
        b.iter(|| {
            let mut arena: BrandedArena<'_, usize, 1024> = BrandedArena::new();
            let mut keys = Vec::new();
            for i in 0..1000 {
                keys.push(arena.alloc(i));
            }
            black_box(keys.len());
        });
    });

    c.bench_function("branded_arena_access", |b| {
        GhostToken::new(|mut token| {
            let mut arena: BrandedArena<'_, usize, 1024> = BrandedArena::new();
            let mut keys = Vec::new();
            for i in 0..1000 {
                keys.push(arena.alloc(i));
            }
            b.iter(|| {
                for &key in &keys {
                    black_box(arena.get_key(&token, key));
                    *arena.get_key_mut(&mut token, key) += 1;
                }
            });
        });
    });
}

criterion_group!(
    benches,
    bench_branded_chunked_vec_32,
    bench_branded_chunked_vec_64,
    bench_branded_chunked_vec_128,
    bench_branded_chunked_vec_256,
    bench_branded_chunked_vec_512,
    bench_branded_chunked_vec_1024,
    bench_branded_vec_operations,
    bench_branded_vec_deque_operations,
    bench_branded_hash_map_operations,
    bench_branded_hash_set_operations,
    bench_branded_arena_operations
);
criterion_main!(benches);