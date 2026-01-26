use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::{GhostCell, GhostToken};
use std::sync::{Arc, RwLock};

// Static cell for benchmarking global access
static GLOBAL_CELL: GhostCell<'static, i32> = GhostCell::new(42);

fn bench_hierarchical_derivation(c: &mut Criterion) {
    c.bench_function("hierarchy_split_immutable", |b| {
        GhostToken::new(|token| {
            b.iter(|| {
                black_box(token.split_immutable());
            })
        });
    });

    c.bench_function("hierarchy_static_child", |b| {
        b.iter(|| {
            black_box(halo::token::global::static_child_token());
        })
    });
}

fn bench_concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_reads");

    // RwLock baseline
    {
        let lock = Arc::new(RwLock::new(42));
        group.bench_function("rwlock_read", |b| {
            b.iter(|| {
                let _guard = lock.read().unwrap();
                black_box(*_guard)
            });
        });
    }

    // Hierarchical Ghost Token (Static Child)
    {
        group.bench_function("static_child_read", |b| {
            // Deriving token once per iteration mimics usage in a loop or function
            b.iter(|| {
                let token = halo::token::global::static_child_token();
                let val = GLOBAL_CELL.borrow(&token);
                black_box(*val)
            });
        });
    }

    // Hierarchical Ghost Token (Pre-derived)
    {
        let token = halo::token::global::static_child_token();
        group.bench_function("static_child_read_prederived", |b| {
            b.iter(|| {
                let val = GLOBAL_CELL.borrow(&token);
                black_box(*val)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_hierarchical_derivation, bench_concurrent_reads);
criterion_main!(benches);
