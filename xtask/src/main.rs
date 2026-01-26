use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Halo workspace automation", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run comparative benchmarks
    Bench {
        /// Run quickly (lower sample size/time)
        #[arg(long, default_value_t = false)]
        quick: bool,

        /// Generate report only (skip running benchmarks)
        #[arg(long, default_value_t = false)]
        report_only: bool,
    },
}

const ALLOCATORS: &[&str] = &[
    "alloc-system",
    "alloc-halo",
    "alloc-mimalloc",
    "alloc-snmalloc",
    "alloc-jemalloc"
];

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Bench { quick, report_only } => {
            if !report_only {
                run_benchmarks(quick)?;
            }
            generate_report()?;
        }
    }

    Ok(())
}

fn run_benchmarks(quick: bool) -> Result<()> {
    println!("Running comparative benchmarks...");

    // Build first to avoid measuring build time
    println!("Compiling benchmarks...");
    let status = Command::new("cargo")
        .args(["build", "--bench", "suite", "--release"])
        .status()?;
    if !status.success() {
        anyhow::bail!("Failed to compile benchmarks");
    }

    for alloc in ALLOCATORS {
        println!("\n>>> Benchmarking with feature: {}", alloc);
        let start = Instant::now();

        let baseline_name = alloc.replace("alloc-", "");

        let mut cmd = Command::new("cargo");
        cmd.env("CARGO_INCREMENTAL", "0")
           .env("RUSTFLAGS", "-C opt-level=3 -C codegen-units=1");

        cmd.arg("bench")
            .arg("--bench")
            .arg("suite")
            .arg("--features")
            .arg(alloc)
            .arg("--no-default-features");

        // Args for the test runner (Criterion) go after --
        cmd.arg("--");
        cmd.arg("--save-baseline").arg(&baseline_name);

        if quick {
            // Ultra aggressive settings for CI/Sandbox to avoid timeouts
            cmd.arg("--measurement-time").arg("0.1");
            // cmd.arg("--warmup-time").arg("0.1"); // Not supported by CLI
            cmd.arg("--noplot");
            cmd.arg("--sample-size").arg("10");
        }

        let status = cmd.status().context(format!("Failed to run bench for {}", alloc))?;

        if !status.success() {
            eprintln!("Warning: Benchmark failed for {}", alloc);
        } else {
            println!("Finished {} in {:.2?}", alloc, start.elapsed());
        }
    }

    Ok(())
}

fn generate_report() -> Result<()> {
    println!("\n>>> Generating Report...");
    let mut results: HashMap<String, HashMap<String, f64>> = HashMap::new();

    let criterion_dir = Path::new("target/criterion");
    if !criterion_dir.exists() {
        eprintln!("No criterion output found at {}", criterion_dir.display());
        return Ok(());
    }

    collect_results(criterion_dir, &mut results);

    let report_path = Path::new("benchmark_results/report.md");
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }

    use std::io::Write;
    let mut file = fs::File::create(report_path)?;

    writeln!(file, "# Comparative Benchmark Report")?;

    // Sort workloads
    let mut workloads: Vec<_> = results.keys().collect();
    workloads.sort();

    // Header
    write!(file, "| Workload |")?;
    for alloc in ALLOCATORS {
         let name = alloc.replace("alloc-", "");
         write!(file, " {} (Ops/s) | vs System |", name)?;
    }
    writeln!(file)?;

    // Separator
    write!(file, "|---|")?;
    for _ in ALLOCATORS {
        write!(file, "---|---|")?;
    }
    writeln!(file)?;

    // Rows
    for workload in workloads {
        write!(file, "| {} |", workload)?;

        let system_ops = results.get(workload)
            .and_then(|m| m.get("system"))
            .copied()
            .unwrap_or(0.0);

        for alloc in ALLOCATORS {
            let name = alloc.replace("alloc-", "");
            if let Some(ops) = results.get(workload).and_then(|m| m.get(&name)) {
                let rel = if system_ops > 0.0 { ops / system_ops } else { 0.0 };

                let ops_str = if *ops > 1_000_000.0 {
                    format!("{:.2}M", ops / 1_000_000.0)
                } else if *ops > 1_000.0 {
                    format!("{:.2}K", ops / 1_000.0)
                } else {
                    format!("{:.0}", ops)
                };

                write!(file, " {} | **{:.2}x** |", ops_str, rel)?;
            } else {
                write!(file, " N/A | - |")?;
            }
        }
        writeln!(file)?;
    }

    println!("Report written to {}", report_path.display());
    Ok(())
}

fn collect_results(dir: &Path, results: &mut HashMap<String, HashMap<String, f64>>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_results(&path, results);
        } else if path.file_name().and_then(|s| s.to_str()) == Some("estimates.json") {
            // Found data
            // Structure: .../workload/baseline/estimates.json
            if let Some(baseline_dir) = path.parent() {
                let baseline_name = baseline_dir.file_name().unwrap().to_str().unwrap().to_string();
                if let Some(workload_dir) = baseline_dir.parent() {
                    let workload_name = workload_dir.file_name().unwrap().to_str().unwrap().to_string();

                    // Filter out 'report' directory or others
                    if baseline_name == "report" || workload_name == "report" {
                        continue;
                    }

                    // Get throughput from benchmark.json
                    let mut elements = 1.0;
                    let mut is_throughput = false;
                    let bench_json = workload_dir.join("benchmark.json");
                    if let Ok(content) = fs::read_to_string(&bench_json) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(t) = json.get("throughput").and_then(|t| t.get("Elements")) {
                                elements = t.as_f64().unwrap_or(1.0);
                                is_throughput = true;
                            }
                        }
                    }

                    // Get time
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(mean) = json.get("mean").and_then(|m| m.get("point_estimate")) {
                                let time_ns = mean.as_f64().unwrap_or(0.0);
                                if time_ns > 0.0 {
                                    let metric = if is_throughput {
                                        (elements * 1e9) / time_ns
                                    } else {
                                        1e9 / time_ns
                                    };

                                    results.entry(workload_name)
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
