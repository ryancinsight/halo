use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, black_box};
use halo::{GhostToken, BrandedMatrix};

fn matrix_access_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Matrix Access");
    let rows = 1000;
    let cols = 1000;

    // 1. BrandedMatrix
    group.bench_function("BrandedMatrix::get", |b| {
        GhostToken::new(|token| {
            let mat = BrandedMatrix::<i32>::new(rows, cols);
            b.iter(|| {
                let r = black_box(500);
                let c = black_box(500);
                black_box(mat.get(&token, r, c));
            });
        });
    });

    // 2. Vec<Vec<T>>
    group.bench_function("Vec<Vec<T>>::index", |b| {
        let mut vec = Vec::with_capacity(rows);
        for _ in 0..rows {
            vec.push(vec![0; cols]);
        }
        b.iter(|| {
            let r = black_box(500);
            let c = black_box(500);
            black_box(&vec[r][c]);
        });
    });

    // 3. Flattened Vec<T>
    group.bench_function("Vec<T>::index_calculated", |b| {
        let vec = vec![0; rows * cols];
        b.iter(|| {
            let r = black_box(500);
            let c = black_box(500);
            black_box(&vec[r * cols + c]);
        });
    });

    group.finish();
}

fn matrix_iteration_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Matrix Iteration");
    let rows = 500;
    let cols = 500;

    // 1. BrandedMatrix (row iteration)
    group.bench_function("BrandedMatrix::iter_rows", |b| {
        GhostToken::new(|token| {
            let mat = BrandedMatrix::<i32>::new(rows, cols);
            b.iter(|| {
                let mut sum = 0;
                for r in 0..rows {
                    if let Some(row) = mat.row(&token, r) {
                        for val in row.as_slice() {
                            sum += *val;
                        }
                    }
                }
                black_box(sum);
            });
        });
    });

    // 2. Vec<Vec<T>>
    group.bench_function("Vec<Vec<T>>::iter", |b| {
        let mut vec = Vec::with_capacity(rows);
        for _ in 0..rows {
            vec.push(vec![0; cols]);
        }
        b.iter(|| {
            let mut sum = 0;
            for row in &vec {
                for val in row {
                    sum += *val;
                }
            }
            black_box(sum);
        });
    });

    // 3. Flattened Vec<T>
    group.bench_function("Vec<T>::iter", |b| {
        let vec = vec![0; rows * cols];
        b.iter(|| {
            let mut sum = 0;
            for val in &vec {
                sum += *val;
            }
            black_box(sum);
        });
    });

    group.finish();
}

fn matrix_split_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Matrix Split Mutation");
    let rows = 1000;
    let cols = 1000;

    // 1. BrandedMatrix Quadrants
    group.bench_function("BrandedMatrix::split_quadrants", |b| {
        GhostToken::new(|mut token| {
            let mut mat = BrandedMatrix::<i32>::new(rows, cols);
            b.iter(|| {
                let view = mat.view_mut();
                let (tl, tr, bl, br) = view.split_quadrants(rows/2, cols/2);

                // Simulate partial work on quadrants
                let mut work = 0;
                // Just check one element from each to simulate access
                // Note: we can't easily iterate all efficiently without iterators on view yet
                // But we can check bounds overhead
                black_box(tl.rows());
                black_box(tr.rows());
                black_box(bl.rows());
                black_box(br.rows());
            });
        });
    });

    // 2. Vec<Vec<T>> Split (slice::split_at_mut)
    group.bench_function("Vec<Vec<T>>::split_at_mut", |b| {
        let mut vec = Vec::with_capacity(rows);
        for _ in 0..rows {
            vec.push(vec![0; cols]);
        }
        b.iter(|| {
            let (top, bottom) = vec.split_at_mut(rows/2);
            // We can't split columns easily in Vec<Vec> without iterating rows!
            // This demonstrates the advantage of BrandedMatrix view.
            black_box(top.len());
            black_box(bottom.len());
        });
    });

    group.finish();
}

criterion_group!(benches, matrix_access_benchmark, matrix_iteration_benchmark, matrix_split_benchmark);
criterion_main!(benches);
