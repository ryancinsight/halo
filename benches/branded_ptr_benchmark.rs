use criterion::{black_box, criterion_group, criterion_main, Criterion};
use halo::foundation::ghost::ptr::BrandedNonNull;
use halo::GhostToken;
use std::cell::RefCell;

fn bench_branded_ptr(c: &mut Criterion) {
    let mut group = c.benchmark_group("Pointer Dereference");

    // Standard raw pointer dereference
    group.bench_function("Raw Pointer", |b| {
        let mut val = 42;
        let ptr = &mut val as *mut i32;
        b.iter(|| {
            // Safety: ptr is valid
            unsafe { *black_box(ptr) += 1; }
        })
    });

    // RefCell borrow_mut + deref
    group.bench_function("RefCell", |b| {
        let cell = RefCell::new(42);
        b.iter(|| {
            *black_box(cell.borrow_mut()) += 1;
        })
    });

    // BrandedNonNull borrow_mut (with token)
    group.bench_function("BrandedNonNull", |b| {
        GhostToken::new(|mut token| {
            let mut val = 42;
            // Safety: ptr is valid
            let ptr = unsafe { BrandedNonNull::new_unchecked(&mut val as *mut i32) };
            b.iter(|| {
                // Safety: ptr is valid, token is unique
                unsafe { *black_box(ptr.borrow_mut(&mut token)) += 1; }
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_branded_ptr);
criterion_main!(benches);
