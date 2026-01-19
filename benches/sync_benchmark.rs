use criterion::{criterion_group, criterion_main, Criterion, black_box};
use halo::concurrency::sync::{GhostMutex};
use halo::GhostToken;
use std::sync::{Arc, Mutex};
use std::thread;

fn bench_mutex(c: &mut Criterion) {
    let mut group = c.benchmark_group("mutex");

    group.bench_function("std_mutex_uncontended", |b| {
        let mutex = Mutex::new(0);
        b.iter(|| {
            let mut guard = mutex.lock().unwrap();
            *guard += 1;
            black_box(*guard);
        })
    });

    group.bench_function("ghost_mutex_uncontended", |b| {
        GhostToken::new(|token| {
            let mutex = GhostMutex::new(token);
            b.iter(|| {
                let mut guard = mutex.lock();
                black_box(&mut *guard);
            })
        })
    });

    group.bench_function("std_mutex_contended", |b| {
        let mutex = Arc::new(Mutex::new(0));
        b.iter(|| {
            let m = mutex.clone();
            thread::scope(|s| {
                let t = s.spawn(move || {
                    for _ in 0..1000 {
                        let mut guard = m.lock().unwrap();
                        *guard += 1;
                    }
                });
                for _ in 0..1000 {
                    let mut guard = mutex.lock().unwrap();
                    *guard += 1;
                }
                t.join().unwrap();
            });
        })
    });

    group.bench_function("ghost_mutex_contended", |b| {
        GhostToken::new(|token| {
             let mutex = Arc::new(GhostMutex::new(token));
             b.iter(|| {
                 let m = mutex.clone();
                 thread::scope(|s| {
                     let t = s.spawn(move || {
                         for _ in 0..1000 {
                             let mut guard = m.lock();
                             black_box(&mut *guard);
                         }
                     });
                     for _ in 0..1000 {
                         let mut guard = mutex.lock();
                         black_box(&mut *guard);
                     }
                     t.join().unwrap();
                 });
             })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_mutex);
criterion_main!(benches);
