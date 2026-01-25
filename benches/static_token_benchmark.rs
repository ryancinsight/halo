use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::token::{static_token, with_static_token, GhostToken};
use std::thread;

fn bench_static_token_access(c: &mut Criterion) {
    c.bench_function("static_token_acquire", |b| {
        b.iter(|| {
            // Measure cost of calling static_token().
            // This involves a OnceLock check.
            let token = static_token();
            black_box(token);
        });
    });

    c.bench_function("ghost_token_new_overhead", |b| {
        b.iter(|| {
            // Measure cost of creating a fresh token.
            GhostToken::new(|token| {
                black_box(token);
            });
        });
    });
}

fn bench_static_token_concurrent_access(c: &mut Criterion) {
    c.bench_function("static_token_concurrent_read", |b| {
        b.iter(|| {
            thread::scope(|s| {
                for _ in 0..4 {
                    s.spawn(|| {
                        for _ in 0..1000 {
                            with_static_token(|token| {
                                black_box(token);
                            });
                        }
                    });
                }
            });
        });
    });
}

criterion_group!(
    benches,
    bench_static_token_access,
    bench_static_token_concurrent_access
);
criterion_main!(benches);
