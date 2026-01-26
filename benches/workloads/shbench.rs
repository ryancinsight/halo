use criterion::{black_box, Criterion};

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

const LIVE_BYTES: usize = 1024 * 1024 * 4; // 4MB live set
const OPS: usize = 10_000;

pub fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("shbench");

    group.bench_function("fragmentation_churn", |b| {
        b.iter(|| {
            let mut rng = XorShift64::new(0x1234_5678);
            let mut live_data = Vec::new();
            let mut current_bytes = 0;

            // Phase 1: Build Live Set
            while current_bytes < LIVE_BYTES {
                let size = rng.gen_range(16, 8192);
                let v = vec![0u8; size];
                current_bytes += size;
                live_data.push(v);
            }

            // Phase 2: Random Churn
            for _ in 0..OPS {
                let idx = rng.gen_range(0, live_data.len());
                let old_len = live_data[idx].len();

                let new_size = rng.gen_range(16, 8192);
                let v = vec![0u8; new_size];

                // Replace
                live_data[idx] = v;

                current_bytes = current_bytes + new_size - old_len;
                black_box(&live_data[idx]);
            }

            black_box(live_data);
        })
    });

    group.finish();
}
