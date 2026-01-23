use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::other::{BrandedBitSet, BrandedBloomFilter};
use halo::GhostToken;
use std::collections::HashSet;
use std::mem;

fn bit_set_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("BitSet vs HashSet");

    let size = 10_000;

    // Insert
    // For insert, we create the set in setup.
    group.bench_function("BrandedBitSet insert", |b| {
        b.iter_batched(
            || unsafe {
                // Create with 'static lifetime for transport
                mem::transmute::<BrandedBitSet<'_>, BrandedBitSet<'static>>(
                    BrandedBitSet::with_capacity(size * 64),
                )
            },
            |set| {
                GhostToken::new(|mut token| {
                    // Cast to current token
                    let mut set =
                        unsafe { mem::transmute::<BrandedBitSet<'static>, BrandedBitSet<'_>>(set) };
                    for i in 0..size {
                        set.insert(&mut token, black_box(i * 3));
                    }
                });
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("std::HashSet insert", |b| {
        b.iter_batched(
            || HashSet::with_capacity(size),
            |mut set| {
                for i in 0..size {
                    set.insert(black_box(i * 3));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Contains
    group.bench_function("BrandedBitSet contains", |b| {
        // We can just build it once if we don't need iter_batched (read only)
        // But GhostToken scope limits us.
        // We have to put the whole setup inside GhostToken::new, but benchmark loop inside.
        // Or build it, transmute to static, then in benchmark loop transmute back.

        // Build once
        let set_static = GhostToken::new(|mut token| {
            let mut set = BrandedBitSet::with_capacity(size * 64);
            for i in 0..size {
                set.insert(&mut token, i * 3);
            }
            unsafe { mem::transmute::<BrandedBitSet<'_>, BrandedBitSet<'static>>(set) }
        });

        b.iter(|| {
            GhostToken::new(|token| {
                let set = unsafe {
                    mem::transmute::<&BrandedBitSet<'static>, &BrandedBitSet<'_>>(&set_static)
                };
                for i in 0..size {
                    black_box(set.contains(&token, black_box(i * 3)));
                }
            })
        });
    });

    group.bench_function("std::HashSet contains", |b| {
        let mut set = HashSet::with_capacity(size);
        for i in 0..size {
            set.insert(i * 3);
        }

        b.iter(|| {
            for i in 0..size {
                black_box(set.contains(black_box(&(i * 3))));
            }
        })
    });

    // Union
    group.bench_function("BrandedBitSet union", |b| {
        b.iter_batched(
            || {
                let (s1, s2) = GhostToken::new(|mut token| {
                    let mut set1 = BrandedBitSet::with_capacity(size * 64);
                    let mut set2 = BrandedBitSet::with_capacity(size * 64);
                    for i in 0..size {
                        set1.insert(&mut token, i * 2);
                        set2.insert(&mut token, i * 3);
                    }
                    unsafe {
                        (
                            mem::transmute::<BrandedBitSet<'_>, BrandedBitSet<'static>>(set1),
                            mem::transmute::<BrandedBitSet<'_>, BrandedBitSet<'static>>(set2),
                        )
                    }
                });
                (s1, s2)
            },
            |(s1, s2)| {
                GhostToken::new(|mut token| {
                    let mut set1 =
                        unsafe { mem::transmute::<BrandedBitSet<'static>, BrandedBitSet<'_>>(s1) };
                    let set2 =
                        unsafe { mem::transmute::<BrandedBitSet<'static>, BrandedBitSet<'_>>(s2) };

                    set1.union_with(&mut token, &set2);
                });
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("std::HashSet union", |b| {
        let mut set1 = HashSet::with_capacity(size);
        let mut set2 = HashSet::with_capacity(size);
        for i in 0..size {
            set1.insert(i * 2);
            set2.insert(i * 3);
        }

        b.iter_batched(
            || (set1.clone(), set2.clone()),
            |(mut s1, s2)| {
                s1.extend(&s2);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bloom_filter_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("BloomFilter vs HashSet");

    let size = 10_000;

    // Insert
    group.bench_function("BrandedBloomFilter insert", |b| {
        b.iter_batched(
            || unsafe {
                mem::transmute::<BrandedBloomFilter<usize>, BrandedBloomFilter<'static, usize>>(
                    BrandedBloomFilter::<usize>::with_capacity_and_fp_rate(size, 0.01),
                )
            },
            |bloom| {
                GhostToken::new(|mut token| {
                    let mut bloom = unsafe {
                        mem::transmute::<
                            BrandedBloomFilter<'static, usize>,
                            BrandedBloomFilter<'_, usize>,
                        >(bloom)
                    };
                    for i in 0..size {
                        bloom.insert(&mut token, black_box(&i));
                    }
                });
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Contains
    group.bench_function("BrandedBloomFilter contains", |b| {
        let bloom_static = GhostToken::new(|mut token| {
            let mut bloom = BrandedBloomFilter::<usize>::with_capacity_and_fp_rate(size, 0.01);
            for i in 0..size {
                bloom.insert(&mut token, &i);
            }
            unsafe {
                mem::transmute::<BrandedBloomFilter<usize>, BrandedBloomFilter<'static, usize>>(
                    bloom,
                )
            }
        });

        b.iter(|| {
            GhostToken::new(|token| {
                let bloom = unsafe {
                    mem::transmute::<
                        &BrandedBloomFilter<'static, usize>,
                        &BrandedBloomFilter<'_, usize>,
                    >(&bloom_static)
                };
                for i in 0..size {
                    black_box(bloom.contains(&token, black_box(&i)));
                }
            })
        });
    });

    // Compare with HashSet for contains
    group.bench_function("std::HashSet contains", |b| {
        let mut set = HashSet::with_capacity(size);
        for i in 0..size {
            set.insert(i);
        }

        b.iter(|| {
            for i in 0..size {
                black_box(set.contains(black_box(&i)));
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bit_set_benchmark, bloom_filter_benchmark);
criterion_main!(benches);
