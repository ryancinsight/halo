//! Criterion ratio report + regression gate.
//!
//! Workflow:
//! 1) `cargo bench`
//! 2) `cargo run --example bench_report -- --threshold 1.05`
//!
//! This parses `target/criterion/**/new/estimates.json` (Criterion output) and computes
//! Ghost-vs-stdlib ratios within the same run.

use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process,
};

#[derive(Debug, Clone)]
struct Estimate {
    mean_point_estimate_ns: f64,
    median_point_estimate_ns: f64,
}

#[derive(Copy, Clone, Debug)]
enum Stat {
    Mean,
    Median,
}

fn main() {
    let mut args = env::args().skip(1);
    let mut criterion_dir: Option<PathBuf> = None;
    let mut threshold: f64 = 1.05;
    let mut stat: Stat = Stat::Mean;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--criterion-dir" => {
                let p = args.next().unwrap_or_else(|| usage_exit("missing value for --criterion-dir"));
                criterion_dir = Some(PathBuf::from(p));
            }
            "--threshold" => {
                let v = args.next().unwrap_or_else(|| usage_exit("missing value for --threshold"));
                threshold = v.parse::<f64>().unwrap_or_else(|_| usage_exit("invalid float for --threshold"));
            }
            "--stat" => {
                let v = args.next().unwrap_or_else(|| usage_exit("missing value for --stat"));
                stat = match v.as_str() {
                    "mean" => Stat::Mean,
                    "median" => Stat::Median,
                    _ => usage_exit("invalid value for --stat (expected: mean|median)"),
                };
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            other => usage_exit(&format!("unknown argument: {other}")),
        }
    }

    let criterion_dir = criterion_dir.unwrap_or_else(|| PathBuf::from("target").join("criterion"));
    if threshold.is_nan() || threshold < 1.0 {
        usage_exit("--threshold must be a finite float >= 1.0");
    }

    let estimates = read_all_estimates(&criterion_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to read criterion output: {e}");
        process::exit(2);
    });

    // Pairs: (ghost_key, std_key, label)
    let comparisons: &[(&str, &str, &str)] = &[
        (
            "ghost_unsafe_cell_get_mut_loop",
            "unsafe_cell_get_mut",
            "GhostUnsafeCell get_mut loop vs UnsafeCell get_mut loop",
        ),
        ("ghostcell_copy_ops_wrapping", "refcell_copy_ops_wrapping", "GhostCell inc loop vs RefCell inc loop"),
        ("ghost_lazy_lock_cached_get", "std_sync_lazy_lock_cached_get", "GhostLazyLock cached get vs std::sync::LazyLock cached get"),
        ("ghost_lazy_lock_first_get", "std_sync_lazy_lock_first_get", "GhostLazyLock first get vs std::sync::LazyLock first get"),
        ("ghost_once_cell_get", "std_once_cell_get", "GhostOnceCell get vs std::cell::OnceCell get"),
        ("ghost_once_cell_set", "std_once_cell_set", "GhostOnceCell set vs std::cell::OnceCell set"),
        ("ghost_atomic_u64_fetch_add", "std_atomic_u64_fetch_add", "GhostAtomicU64 fetch_add vs AtomicU64 fetch_add"),
        ("ghost_chunked_vec_push_iter_sum", "std_vec_push_iter_sum", "ChunkedVec push+iter sum vs Vec push+iter sum"),
        (
            "ghost_csr_dfs_atomic_visited",
            "std_csr_dfs_atomicbool_visited",
            "Ghost CSR DFS (atomic visited) vs std CSR DFS (AtomicBool visited)",
        ),
        ("ghost_scoped_read_fanout", "std_rwlock_read_fanout", "Scoped fan-out read: GhostToken vs RwLock"),
        (
            "ghost_scoped_many_cells_read",
            "std_rwlock_many_cells_read",
            "Many-cells read: scoped GhostToken vs per-cell RwLock",
        ),
        (
            "ghost_scoped_many_cells_write_batched_commit",
            "std_rwlock_many_cells_write",
            "Many-cells write: batched commit (GhostToken) vs per-cell RwLock write",
        ),
        ("ghost_scoped_baton_write", "std_mutex_baton_write", "Scoped baton write: GhostToken vs Mutex"),
        (
            "ghost_parallel_reachable_lockfree_worklist",
            "std_parallel_reachable_mutex_worklist",
            "Parallel reachability: lock-free worklist vs Mutex worklist",
        ),
        (
            "ghost_parallel_reachable_lockfree_worklist_batched",
            "std_parallel_reachable_mutex_worklist",
            "Parallel reachability (batched): lock-free batched vs Mutex worklist",
        ),
        (
            "ghost_parallel_reachable_lockfree_worklist_batched_bitset",
            "ghost_parallel_reachable_lockfree_worklist_batched",
            "Parallel reachability: bitset visited vs AtomicBool visited (both lock-free batched)",
        ),
        (
            "ghost_parallel_reachable_workstealing_deque_hi",
            "ghost_parallel_reachable_lockfree_worklist_batched_hi",
            "High-contention reachability: Chase–Lev deque vs batched Treiber stack",
        ),
        ("branded_vec_push_pop", "std_vec_push_pop", "BrandedVec vs std::vec::Vec (push/pop)"),
        ("branded_vec_deque_push_pop", "std_vec_deque_push_pop", "BrandedVecDeque vs std::collections::VecDeque (push/pop)"),
        ("branded_hash_map_insert_get", "std_hash_map_insert_get", "BrandedHashMap vs std::collections::HashMap (insert/get)"),
    ];

    let mut missing = Vec::new();
    for (ghost, std, _) in comparisons {
        if !estimates.contains_key(*ghost) {
            missing.push(*ghost);
        }
        if !estimates.contains_key(*std) {
            missing.push(*std);
        }
    }

    if !missing.is_empty() {
        eprintln!("error: missing benchmark estimates in `{}`:", criterion_dir.display());
        for name in missing {
            eprintln!("  - {name}");
        }
        eprintln!("\nTip: run `cargo bench` first. If you renamed benches, update `bench_report.rs`.");
        process::exit(2);
    }

    println!("Criterion dir: {}", criterion_dir.display());
    println!("Threshold:     {:.4} (Ghost/std must be <= threshold)\n", threshold);

    let stat_name = match stat {
        Stat::Mean => "mean",
        Stat::Median => "median",
    };

    println!("Stat:          {stat_name}\n");

    println!("{:<58} {:>12} {:>12} {:>10}", "comparison", "ghost(ns)", "std(ns)", "ratio");
    println!("{:-<96}", "");

    let mut failed = false;
    for (ghost, std, label) in comparisons {
        let eg = estimates.get(*ghost).unwrap();
        let es = estimates.get(*std).unwrap();
        let g = match stat {
            Stat::Mean => eg.mean_point_estimate_ns,
            Stat::Median => eg.median_point_estimate_ns,
        };
        let s = match stat {
            Stat::Mean => es.mean_point_estimate_ns,
            Stat::Median => es.median_point_estimate_ns,
        };
        let ratio = g / s;

        println!("{:<58} {:>12.6} {:>12.6} {:>10.4}", label, g, s, ratio);

        if ratio.is_nan() || ratio > threshold {
            failed = true;
        }
    }

    if failed {
        eprintln!("\nFAIL: at least one Ghost/std ratio exceeded threshold {:.4}.", threshold);
        process::exit(1);
    }

    println!("\nOK: all Ghost/std ratios are within threshold {:.4}.", threshold);
}

fn usage() {
    eprintln!(
        "Usage: cargo run --example bench_report -- [--criterion-dir PATH] [--threshold FLOAT] [--stat mean|median]\n\
         \n\
         Defaults:\n\
         - criterion dir: target/criterion\n\
         - threshold:     1.05\n"
    );
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("error: {msg}\n");
    usage();
    process::exit(2)
}

fn read_all_estimates(root: &Path) -> Result<BTreeMap<String, Estimate>, String> {
    let mut out = BTreeMap::new();
    if !root.exists() {
        return Err(format!("criterion directory does not exist: {}", root.display()));
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
        for ent in entries {
            let ent = ent.map_err(|e| format!("read_dir entry {}: {e}", dir.display()))?;
            let path = ent.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name() != Some(OsStr::new("estimates.json")) {
                continue;
            }
            // Criterion layout is typically: <bench>/new/estimates.json
            // We'll accept any .../new/estimates.json and name it by the parent-of-`new`.
            let parent = match path.parent() {
                Some(p) => p,
                None => continue,
            };
            if parent.file_name() != Some(OsStr::new("new")) {
                continue;
            }
            let bench_dir = match parent.parent() {
                Some(p) => p,
                None => continue,
            };
            let bench_name = bench_dir
                .strip_prefix(root)
                .unwrap_or(bench_dir)
                .to_string_lossy()
                .replace('\\', "/");

            let json = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let est = parse_estimates_json(&json).ok_or_else(|| format!("failed to parse {}", path.display()))?;
            out.insert(bench_name, est);
        }
    }
    Ok(out)
}

fn parse_estimates_json(s: &str) -> Option<Estimate> {
    // Criterion estimates are like:
    // {"mean":{"confidence_interval":...,"point_estimate":0.7247,...}, ...}
    let mean_point = find_point_estimate(s, "\"mean\"")?;
    let median_point = find_point_estimate(s, "\"median\"")?;
    Some(Estimate {
        mean_point_estimate_ns: mean_point,
        median_point_estimate_ns: median_point,
    })
}

fn find_point_estimate(s: &str, section_key: &str) -> Option<f64> {
    let sec = s.find(section_key)?;
    let point = s[sec..].find("\"point_estimate\"")? + sec;
    let colon = s[point..].find(':')? + point;
    let (v, _) = parse_f64(&s[colon + 1..])?;
    Some(v)
}

fn parse_f64(s: &str) -> Option<(f64, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
            i += 1;
            continue;
        }
        break;
    }
    if i == start {
        return None;
    }
    let v = s[start..i].parse::<f64>().ok()?;
    Some((v, i))
}


