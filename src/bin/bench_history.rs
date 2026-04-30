//! bench_history — Read all benchmark result JSON files from benchmarks/results/
//! and print a table of benchmark names vs times over time, flagging any
//! regression > 10 % from the previous run.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// JSON schema produced by bench_compare.sh
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize)]
struct BenchRun {
    timestamp: String,
    date: String,
    commit: String,
    branch: String,
    results: Vec<BenchResult>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BenchResult {
    name: String,
    median: String,
    #[allow(dead_code)]
    lower: String,
    #[allow(dead_code)]
    upper: String,
    unit: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse the numeric part out of a value like "4.2345" (already stripped of
/// units by the shell script). Returns None if parsing fails.
fn parse_time(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

/// Load all JSON files from the results directory, sorted by timestamp
/// (filename contains the timestamp so lexicographic sort == chronological).
fn load_results(dir: &Path) -> Vec<(String, BenchRun)> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| {
            eprintln!("Cannot read {}: {e}", dir.display());
            std::process::exit(1);
        })
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                return None;
            }
            let content = fs::read_to_string(&path).ok()?;
            let run: BenchRun = serde_json::from_str(&content).ok()?;
            Some((path.file_name()?.to_string_lossy().to_string(), run))
        })
        .collect();

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let repo_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let results_dir = Path::new(&repo_root).join("benchmarks/results");

    if !results_dir.exists() {
        eprintln!(
            "No benchmark results found at {}.",
            results_dir.display()
        );
        eprintln!("Run ./scripts/bench_compare.sh first to generate results.");
        std::process::exit(1);
    }

    let runs = load_results(&results_dir);

    if runs.is_empty() {
        eprintln!("No JSON result files found in {}.", results_dir.display());
        std::process::exit(1);
    }

    // Collect the set of all benchmark names across all runs (preserving
    // first-seen order).
    let mut bench_names: Vec<String> = Vec::new();
    for (_, run) in &runs {
        for r in &run.results {
            if !bench_names.contains(&r.name) {
                bench_names.push(r.name.clone());
            }
        }
    }

    // Print header
    print!("{:<42}", "benchmark");
    for (filename, _) in &runs {
        // Trim to a readable date portion from filename like bench_20250430T120000Z.json
        let label = filename
            .trim_start_matches("bench_")
            .trim_end_matches(".json");
        print!("  {:>18}", label);
    }
    println!();
    println!("{}", "-".repeat(42 + 20 * runs.len()));

    // For regression tracking: per-benchmark, remember the last median seen.
    let mut last_median: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut regressions: Vec<String> = Vec::new();

    for name in &bench_names {
        print!("{:<42}", name);
        for (_, run) in &runs {
            if let Some(result) = run.results.iter().find(|r| r.name == *name) {
                let cell = format!("{} {}", result.median, result.unit);
                print!("  {:>18}", cell);

                // Regression check
                if let Some(current) = parse_time(&result.median) {
                    if let Some(&prev) = last_median.get(name)
                        && prev > 0.0
                    {
                        let change = (current - prev) / prev;
                        if change > 0.10 {
                            regressions.push(format!(
                                "  {} regressed +{:.1}% ({:.4} → {:.4}) in run {}",
                                name,
                                change * 100.0,
                                prev,
                                current,
                                run.date,
                            ));
                        }
                    }
                    last_median.insert(name.clone(), current);
                }
            } else {
                print!("  {:>18}", "—");
            }
        }
        println!();
    }

    println!();

    if regressions.is_empty() {
        println!("✅ No regressions (> 10%) detected across runs.");
    } else {
        println!("⚠️  Regressions (> 10% from previous run):");
        for msg in &regressions {
            println!("{msg}");
        }
        std::process::exit(2);
    }
}
