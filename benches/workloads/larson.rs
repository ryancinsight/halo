use criterion::{black_box, Criterion, Throughput};
use std::thread;

const OPS_PER_THREAD: usize = 50_000;
const OBJECTS_PER_THREAD: usize = 1000;
const MIN_SIZE: usize = 16;
const MAX_SIZE: usize = 128; // Small allocations typical of objects

struct XorShift64 {
    a: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { a: if seed == 0 { 1 } else { seed } }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.a;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.a = x;
        x
    }

    fn gen_range(&mut self, min: usize, max: usize) -> usize {
        (self.next() as usize % (max - min)) + min
    }
}

pub fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("larson");

    // Test concurrency levels
    let threads = [1, 2, 4, 8, 16];

    for &t in &threads {
        group.throughput(Throughput::Elements((OPS_PER_THREAD * t) as u64));
        group.bench_function(format!("larson_{}_threads", t), |b| {
            b.iter(|| {
                let mut handles = Vec::with_capacity(t);
                for i in 0..t {
                    handles.push(thread::spawn(move || {
                        let mut rng = XorShift64::new((i as u64 + 1) * 0xdead_beef);
                        let mut objects = Vec::with_capacity(OBJECTS_PER_THREAD);
                        // Initialize
                        for _ in 0..OBJECTS_PER_THREAD {
                             objects.push(Vec::<u8>::new());
                        }

                        // Main loop
                        for _ in 0..OPS_PER_THREAD {
                            let idx = rng.gen_range(0, OBJECTS_PER_THREAD);
                            let size = rng.gen_range(MIN_SIZE, MAX_SIZE);
                            // Allocate new, drop old (implicitly)
                            objects[idx] = vec![0u8; size];
                            black_box(&objects[idx]);
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
