use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use halo::{GhostToken, collections::{string::{BrandedRope, ActivateRope}}};

fn bench_rope_insert_middle(c: &mut Criterion) {
    let mut group = c.benchmark_group("Rope vs String: Insert Middle");

    // Setup large text: 1MB (10x larger)
    let initial_text = "a".repeat(1_000_000);

    group.bench_function("std::String", |b| {
        b.iter_batched(
            || initial_text.clone(),
            |mut s| {
                // Insert in middle
                s.insert_str(500_000, "HELLO");
                black_box(s);
            },
            BatchSize::SmallInput
        )
    });

    group.bench_function("ActiveRope", |b| {
        GhostToken::new(|mut token| {
            // Workaround for double mutable borrow of token in closures
            let token_ptr = &mut token as *mut GhostToken;

            b.iter_batched(
                || {
                    let token = unsafe { &mut *token_ptr };
                    // Setup: Create a fresh rope with 1MB text
                    // Reserve enough capacity to avoid realloc during insert
                    let mut rope = BrandedRope::with_capacity(2000);
                    rope.append(token, &initial_text);
                    // Reserve a bit more for the insert operation
                    rope.reserve_nodes(10);
                    rope
                },
                |mut rope| {
                    let token = unsafe { &mut *token_ptr };
                    // Measure: Insert in middle
                    let mut active = rope.activate(token);
                    active.insert(500_000, "HELLO");
                    black_box(active);
                },
                BatchSize::SmallInput
            );
        });
    });

    group.finish();
}

fn bench_rope_append(c: &mut Criterion) {
    let mut group = c.benchmark_group("Rope vs String: Append");

    group.bench_function("std::String", |b| {
        b.iter(|| {
            let mut s = String::new();
            for _ in 0..1000 {
                s.push_str("abc");
            }
            black_box(s);
        })
    });

    group.bench_function("ActiveRope", |b| {
         GhostToken::new(|mut token| {
            b.iter(|| {
                let mut rope = BrandedRope::new();
                let mut active = rope.activate(&mut token);
                for _ in 0..1000 {
                    active.append("abc");
                }
                black_box(active);
            })
         });
    });

    group.finish();
}

criterion_group!(benches, bench_rope_insert_middle, bench_rope_append);
criterion_main!(benches);
