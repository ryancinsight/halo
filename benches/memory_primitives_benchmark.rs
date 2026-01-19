use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use halo::alloc::{StaticRc, BrandedBox};
use halo::GhostToken;
use std::rc::Rc;
use std::sync::Arc;

fn bench_static_rc(c: &mut Criterion) {
    let mut group = c.benchmark_group("StaticRc vs Rc/Arc");

    // Creation
    group.bench_function("Rc::new", |b| {
        b.iter(|| {
            black_box(Rc::new(black_box(42)));
        })
    });

    group.bench_function("Arc::new", |b| {
        b.iter(|| {
            black_box(Arc::new(black_box(42)));
        })
    });

    group.bench_function("StaticRc::new", |b| {
        b.iter(|| {
            // StaticRc::new involves Box allocation (now manual alloc)
            black_box(StaticRc::<'_, i32, 1, 1>::new(black_box(42)));
        })
    });

    // Cloning / Splitting
    // We benchmark the cost of getting another handle to the same data.

    group.bench_function("Rc::clone", |b| {
        b.iter_batched(
            || Rc::new(42),
            |rc| {
                let _ = black_box(rc.clone());
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("Arc::clone", |b| {
        b.iter_batched(
            || Arc::new(42),
            |arc| {
                let _ = black_box(arc.clone());
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("StaticRc::split", |b| {
        b.iter_batched(
            || StaticRc::<'_, i32, 10, 10>::new(42),
            |rc| {
                // split consumes self, returns two.
                // This is effectively "cloning" ownership.
                let (r1, r2) = rc.split::<5, 5>();
                black_box((r1, r2));
            },
            BatchSize::SmallInput,
        )
    });

    // Dereference
    let rc = Rc::new(42);
    group.bench_function("Rc deref", |b| {
        b.iter(|| {
            black_box(*rc);
        })
    });

    let arc = Arc::new(42);
    group.bench_function("Arc deref", |b| {
        b.iter(|| {
            black_box(*arc);
        })
    });

    let static_rc = StaticRc::<'_, i32, 1, 1>::new(42);
    group.bench_function("StaticRc deref", |b| {
        b.iter(|| {
            black_box(*static_rc);
        })
    });

    group.finish();
}

fn bench_branded_box(c: &mut Criterion) {
    let mut group = c.benchmark_group("BrandedBox vs Box");

    // Creation
    group.bench_function("Box::new", |b| {
        b.iter(|| {
            black_box(Box::new(black_box(42)));
        })
    });

    group.bench_function("BrandedBox::new", |b| {
        // We include token creation overhead here as it is required context
        b.iter(|| {
            GhostToken::new(|mut _token| {
                // BrandedBox::new no longer takes token
                black_box(BrandedBox::new(black_box(42)));
            })
        })
    });

    // Access

    group.bench_function("Box deref", |b| {
        let bx = Box::new(42);
        b.iter(|| {
            black_box(*bx);
        })
    });

    group.bench_function("BrandedBox borrow", |b| {
        GhostToken::new(|mut token| {
            let bb = BrandedBox::new(42);
            // token is mutable here, but borrow takes &token (immutable).
            // &mut T coerces to &T.
            b.iter(|| {
                black_box(*bb.borrow(&token));
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_static_rc, bench_branded_box);
criterion_main!(benches);
