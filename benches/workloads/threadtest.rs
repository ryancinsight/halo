use criterion::{black_box, Criterion, Throughput};
use std::thread;
use std::sync::mpsc;

const OPS: usize = 50_000;

pub fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("threadtest_prod_cons");

    // Only even numbers of threads (pairs)
    for t in [2, 4, 8, 16] {
        // Total operations = OPS * number of pairs
        let pairs = t / 2;
        group.throughput(Throughput::Elements((OPS * pairs) as u64));

        group.bench_function(format!("threadtest_{}_threads", t), |b| {
            b.iter(|| {
                let mut handles = Vec::with_capacity(t);

                for _ in 0..pairs {
                    let (tx, rx) = mpsc::channel();

                    // Producer
                    handles.push(thread::spawn(move || {
                        for i in 0..OPS {
                            // Allocating Box<usize>
                            let b = Box::new(i);
                            // Sending moves ownership to other thread
                            if tx.send(b).is_err() {
                                break;
                            }
                        }
                    }));

                    // Consumer
                    handles.push(thread::spawn(move || {
                        // Receiving and Dropping = Remote Free
                        while let Ok(val) = rx.recv() {
                            black_box(val);
                        }
                    }));
                }

                for h in handles {
                    h.join().unwrap();
                }
            })
        });
    }
    group.finish();
}
