use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::collections::BrandedInterner;
use halo::GhostToken;

fn bench_interner(c: &mut Criterion) {
    let mut group = c.benchmark_group("interner");

    // Benchmark interning many unique strings to trigger resize
    // We use a pre-allocated vector of strings to minimize string allocation noise during the measurement loop
    let strings: Vec<String> = (0..5000).map(|i| i.to_string()).collect();

    group.bench_function("intern_unique_strings", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut interner = BrandedInterner::new();
                for s in &strings {
                    interner.intern(&token, black_box(s.clone()));
                }
            });
        });
    });

    // Benchmark interning integers - cheaper key, more focus on map mechanics
    group.bench_function("intern_unique_ints", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut interner = BrandedInterner::new();
                for i in 0..5000 {
                    interner.intern(&token, black_box(i));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_interner);
criterion_main!(benches);
