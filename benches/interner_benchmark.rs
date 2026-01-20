use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{BrandedInterner, GhostToken};
use std::collections::HashSet;

fn benchmark_interner(c: &mut Criterion) {
    let mut group = c.benchmark_group("interner");

    // Dataset: 1000 strings with 100 unique values (high duplication)
    let strings: Vec<String> = (0..1000).map(|i| format!("string_{}", i % 100)).collect();
    let strings_ref: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();

    group.bench_function("std_hashset_insert", |b| {
        b.iter(|| {
            let mut set = HashSet::new();
            for s in &strings {
                set.insert(s.clone());
            }
            black_box(set)
        })
    });

    group.bench_function("branded_interner_insert", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut interner = BrandedInterner::new();
                for s in &strings {
                    interner.intern(&token, s.clone());
                }
                black_box(interner);
            })
        })
    });

    group.bench_function("branded_interner_insert_cow", |b| {
        b.iter(|| {
            GhostToken::new(|token| {
                let mut interner = BrandedInterner::new();
                for s in &strings_ref {
                    // Use intern_cow with Borrowed to avoid allocation
                    interner.intern_cow(&token, std::borrow::Cow::Borrowed(s));
                }
                black_box(interner);
            })
        })
    });

    group.finish();
}

criterion_group!(benches, benchmark_interner);
criterion_main!(benches);
