use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{GhostToken, collections::BrandedCowStrings};

fn bench_cow_strings_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("CowStrings Lookup");

    let strings: Vec<String> = (0..1000).map(|i| format!("string_{}", i)).collect();

    group.bench_function("get_by_value", |b| {
        let strings_ref = &strings;
        GhostToken::new(|token| {
            let mut cow_strings = BrandedCowStrings::new();
            for s in strings_ref {
                cow_strings.insert_owned(&token, s.clone());
            }

            b.iter(|| {
                for s in strings_ref {
                    black_box(cow_strings.get_by_value(&token, s));
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cow_strings_lookup);
criterion_main!(benches);
