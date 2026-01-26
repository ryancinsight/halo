use std::process::Command;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::time::Instant;

// Config
const ALLOCATORS: &[&str] = &["alloc-system", "alloc-halo", "alloc-mimalloc", "alloc-snmalloc", "alloc-jemalloc"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let full = args.contains(&"--full".to_string());
    let quick = args.contains(&"--quick".to_string());
    let report_only = args.contains(&"--report-only".to_string());

    let allocators = if full {
        ALLOCATORS
    } else {
        &["alloc-system", "alloc-halo"]
    };

    if !report_only {
        println!("Running benchmarks for: {:?}", allocators);

        // 1. Run Benchmarks
        for alloc in allocators {
            println!("---------------------------------------------------");
            println!("Benchmarking {}...", alloc);
            let start = Instant::now();

            let baseline_name = alloc.replace("alloc-", "");

            let mut cmd = Command::new("cargo");
            cmd.arg("bench")
                .arg("--bench")
                .arg("suite")
                .arg("--features")
                .arg(alloc)
                .arg("--no-default-features")
                .arg("--")
                .arg("--save-baseline")
                .arg(&baseline_name);

        if quick {
            cmd.arg("--measurement-time").arg("1").arg("--sample-size").arg("10");
        }

        let status = cmd.status()
            .expect("Failed to execute cargo bench");

        if !status.success() {
            eprintln!("Benchmark failed for {}", alloc);
        }

            println!("Completed {} in {:.2?}", alloc, start.elapsed());
        }
    }

    // 2. Collect Results
    println!("---------------------------------------------------");
    println!("Collecting results...");
    let mut results: HashMap<String, HashMap<String, f64>> = HashMap::new(); // Workload -> Allocator -> Ops/Sec

    let criterion_dir = Path::new("target/criterion");
    if criterion_dir.exists() {
        collect_results(criterion_dir, &mut results);
    } else {
        eprintln!("No criterion output found!");
        return;
    }

    // 3. Generate Report
    let report_path = "benchmark_results/report.md";
    if let Err(e) = fs::create_dir_all("benchmark_results") {
         eprintln!("Failed to create benchmark_results dir: {}", e);
    }
    generate_markdown(&results, allocators, report_path);
}

fn collect_results(dir: &Path, results: &mut HashMap<String, HashMap<String, f64>>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_results(&path, results);
            } else if path.file_name().and_then(|s| s.to_str()) == Some("estimates.json") {
                // Found estimates.json
                // Expected path: .../func_name/baseline_name/estimates.json
                if let Some(baseline_dir) = path.parent() {
                    let baseline_name = baseline_dir.file_name().unwrap().to_str().unwrap().to_string();

                    if let Some(func_dir) = baseline_dir.parent() {
                         let func_name = func_dir.file_name().unwrap().to_str().unwrap().to_string();

                         // Try to find throughput config in benchmark.json in func_dir
                         let bench_config_path = func_dir.join("benchmark.json");
                         let mut elements: f64 = 1.0;
                         let mut is_throughput = false;

                         if let Ok(content) = fs::read_to_string(&bench_config_path) {
                             if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                                 if let Some(throughput) = json.get("throughput") {
                                     // Format: { "Elements": 50000 } or { "Bytes": ... }
                                     if let Some(e) = throughput.get("Elements") {
                                         elements = e.as_f64().unwrap_or(1.0);
                                         is_throughput = true;
                                     }
                                 }
                             }
                         }

                         // Parse estimates.json
                         if let Ok(content) = fs::read_to_string(&path) {
                             if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                                 if let Some(mean) = json.get("mean") {
                                     if let Some(point_estimate) = mean.get("point_estimate") {
                                         let time_ns = point_estimate.as_f64().unwrap_or(0.0);

                                         // Calculate Metric
                                         // If throughput is set, we want Ops/Sec = Elements * 1e9 / time_ns
                                         // If not, maybe just 1e9 / time_ns (Hz) or just time.
                                         // User wants Ops/Sec.

                                         let metric = if time_ns > 0.0 {
                                             if is_throughput {
                                                 (elements * 1_000_000_000.0) / time_ns
                                             } else {
                                                  // Fallback: Just report 1.0/time (freq) or time?
                                                  // Let's store time and invert display?
                                                  // No, let's normalize to "Score" (Higher is better).
                                                  // For time, higher speed = lower time.
                                                  // But Ops/Sec is standard.
                                                  // Let's assume 1 op if not specified.
                                                  1_000_000_000.0 / time_ns
                                             }
                                         } else {
                                             0.0
                                         };

                                         results.entry(func_name)
                                                .or_default()
                                                .insert(baseline_name, metric);
                                     }
                                 }
                             }
                         }
                    }
                }
            }
        }
    }
}

fn generate_markdown(results: &HashMap<String, HashMap<String, f64>>, allocators: &[&str], path: &str) {
    use std::io::Write;

    let mut file = fs::File::create(path).expect("Failed to create report");

    writeln!(file, "# Comparative Benchmark Report").unwrap();

    // Sort workloads
    let mut workloads: Vec<_> = results.keys().collect();
    workloads.sort();

    // Table Header
    write!(file, "| Workload |").unwrap();
    for alloc in allocators {
         let name = alloc.replace("alloc-", "");
         write!(file, " {} (Ops/s) | vs System |", name).unwrap();
    }
    writeln!(file).unwrap();

    write!(file, "|---|").unwrap();
    for _ in allocators {
        write!(file, "---|---|").unwrap();
    }
    writeln!(file).unwrap();

    for workload in workloads {
        write!(file, "| {} |", workload).unwrap();

        // Find system baseline
        let system_ops = results.get(workload).and_then(|m| m.get("system")).copied().unwrap_or(0.0);

        for alloc in allocators {
            let name = alloc.replace("alloc-", "");
            if let Some(ops) = results.get(workload).and_then(|m| m.get(&name)) {
                let rel = if system_ops > 0.0 { ops / system_ops } else { 0.0 };
                // Format large numbers
                let ops_str = if *ops > 1_000_000.0 {
                    format!("{:.2}M", ops / 1_000_000.0)
                } else if *ops > 1_000.0 {
                    format!("{:.2}K", ops / 1_000.0)
                } else {
                    format!("{:.0}", ops)
                };

                write!(file, " {} | **{:.2}x** |", ops_str, rel).unwrap();
            } else {
                write!(file, " N/A | - |").unwrap();
            }
        }
        writeln!(file).unwrap();
    }

    println!("Report generated at {}", path);
}
