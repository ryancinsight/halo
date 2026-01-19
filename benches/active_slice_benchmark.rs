use criterion::{criterion_group, criterion_main, Criterion};
use halo::collections::vec::ActivateVec;
use halo::{BrandedVec, GhostToken}; // Import trait

fn bench_branded_slice_mut_iter(c: &mut Criterion) {
    c.bench_function("BrandedSliceMut::iter_mut (10k)", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            for i in 0..10_000 {
                vec.push(i);
            }

            b.iter(|| {
                let mut active = vec.activate(&mut token);
                let mut slice = active.as_mut_slice();
                for x in slice.iter_mut() {
                    *x += 1;
                }
            })
        })
    });
}

fn bench_std_vec_iter_mut(c: &mut Criterion) {
    c.bench_function("std::Vec::iter_mut (10k)", |b| {
        let mut vec: Vec<i32> = (0..10_000).collect();
        b.iter(|| {
            for x in vec.iter_mut() {
                *x += 1;
            }
        })
    });
}

fn bench_active_vec_push(c: &mut Criterion) {
    c.bench_function("ActiveVec::push (1k)", |b| {
        // Here we need to reset the vec each time, so we can't reuse the same vec easily without clearing.
        // But if we clear, we measure clearing cost?
        // Or we use iter_batched?
        // If we use iter_batched, we have the brand issue.
        // So we must include setup cost or use a "reset" cost.

        // For push, we start with empty.
        GhostToken::new(|mut token| {
            b.iter(|| {
                // We have to allocate a new vec inside loop if we want to measure push from scratch.
                // This includes allocation cost.
                // std::Vec benchmark should do the same for fairness.
                let mut vec = BrandedVec::new();
                let mut active = vec.activate(&mut token);
                for i in 0..1000 {
                    active.push(i);
                }
            })
        })
    });
}

fn bench_branded_vec_push_manual(c: &mut Criterion) {
    c.bench_function("BrandedVec::push (1k)", |b| {
        GhostToken::new(|mut token| {
            b.iter(|| {
                let mut vec = BrandedVec::new();
                for i in 0..1000 {
                    vec.push(i);
                    // Note: BrandedVec::push doesn't need token, just &mut self.
                    // But we put it here for symmetry.
                }
                // prevent opt
                std::hint::black_box(&vec);
            })
        })
    });
}

fn bench_active_vec_get_mut(c: &mut Criterion) {
    c.bench_function("ActiveVec::get_mut (1k)", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            for i in 0..1000 {
                vec.push(i);
            }

            b.iter(|| {
                let mut active = vec.activate(&mut token);
                for i in 0..1000 {
                    if let Some(x) = active.get_mut(i) {
                        *x += 1;
                    }
                }
            })
        })
    });
}

fn bench_branded_vec_get_mut_manual(c: &mut Criterion) {
    c.bench_function("BrandedVec::get_mut (1k)", |b| {
        GhostToken::new(|mut token| {
            let mut vec = BrandedVec::new();
            for i in 0..1000 {
                vec.push(i);
            }

            b.iter(|| {
                for i in 0..1000 {
                    if let Some(x) = vec.get_mut(&mut token, i) {
                        *x += 1;
                    }
                }
            })
        })
    });
}

criterion_group!(
    benches,
    bench_branded_slice_mut_iter,
    bench_std_vec_iter_mut,
    bench_active_vec_push,
    bench_branded_vec_push_manual,
    bench_active_vec_get_mut,
    bench_branded_vec_get_mut_manual
);
criterion_main!(benches);
