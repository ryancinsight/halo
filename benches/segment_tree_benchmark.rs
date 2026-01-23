use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use halo::{BrandedSegmentTree, GhostToken};

fn bench_segment_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("Segment Tree");

    let n = 10_000;

    group.bench_function("build", |b| {
        b.iter_batched(
            || (0..n).collect::<Vec<_>>(),
            |data| {
                GhostToken::new(|mut token| {
                    let mut st = BrandedSegmentTree::new(n, |a, b| a + b, 0);
                    st.build(&mut token, &data);
                    black_box(st);
                });
            },
            BatchSize::SmallInput,
        );
    });

    // Expensive clone benchmark
    group.bench_function("build_expensive", |b| {
        b.iter_batched(
            || (0..n).map(|i| vec![i; 64]).collect::<Vec<_>>(),
            |data| {
                GhostToken::new(|mut token| {
                    let mut st = BrandedSegmentTree::new(
                        n,
                        |a, b| {
                            let mut res = Vec::with_capacity(a.len());
                            for (x, y) in a.iter().zip(b.iter()) {
                                res.push(x + y);
                            }
                            res
                        },
                        vec![0; 64],
                    );
                    st.build(&mut token, &data);
                    black_box(st);
                });
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("update", |b| {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(n, |a, b| a + b, 0);
            let data: Vec<_> = (0..n).collect();
            st.build(&mut token, &data);

            b.iter(|| {
                st.update(&mut token, black_box(n / 2), black_box(100));
            });
        });
    });

    group.bench_function("query", |b| {
        GhostToken::new(|mut token| {
            let mut st = BrandedSegmentTree::new(n, |a, b| a + b, 0);
            let data: Vec<_> = (0..n).collect();
            st.build(&mut token, &data);

            b.iter(|| {
                st.query(&token, black_box(n / 4), black_box(3 * n / 4));
            });
        });
    });

    group.bench_function("query_expensive", |b| {
        GhostToken::new(|mut token| {
            let n_exp = 1000;
            // Setup expensive tree
            let data: Vec<_> = (0..n_exp).map(|i| vec![(i % 255) as u8; 1024]).collect();
            let default_val = vec![0u8; 1024];
            let mut st = BrandedSegmentTree::new(
                n_exp,
                |a, b| {
                    let mut res = Vec::with_capacity(a.len());
                    res.extend_from_slice(a);
                    if !b.is_empty() {
                        for i in 0..res.len() {
                            res[i] = res[i].wrapping_add(b[i]);
                        }
                    }
                    res
                },
                default_val,
            );
            st.build(&mut token, &data);

            b.iter(|| {
                // Query range [n/4, 3n/4]
                st.query(&token, black_box(n_exp / 4), black_box(3 * n_exp / 4));
            });
        });
    });

    // Comparison with naive Vec (sum query)
    group.bench_function("naive_vec_query", |b| {
        let data: Vec<usize> = (0..n).collect();
        b.iter(|| {
            let start = black_box(n / 4);
            let end = black_box(3 * n / 4);
            let sum: usize = data[start..end].iter().sum();
            sum
        });
    });

    group.finish();
}

criterion_group!(benches, bench_segment_tree);
criterion_main!(benches);
