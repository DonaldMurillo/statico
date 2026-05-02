#!/usr/bin/env -S cargo run --example ncd_bench --
//! NCD duplicate detection benchmark against statico's own source.
//!
//! Compares NCD candidates vs the existing block/fragment duplication detector.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

fn main() {
    let _root = Path::new(".");
    println!("=== NCD vs Block/Fragment Duplicate Detection Benchmark ===\n");

    // Collect Rust source files from src/
    let mut file_sources: Vec<(String, String)> = Vec::new();
    for entry in walkdir::WalkDir::new("src").into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && let Ok(content) = std::fs::read_to_string(path)
        {
            let rel = path.to_string_lossy().to_string();
            file_sources.push((rel, content));
        }
    }
    println!("Loaded {} source files", file_sources.len());

    // --- NCD detection (threshold 0.5) ---
    let start = Instant::now();
    let ncd_candidates = statico::duplication::find_candidate_pairs(&file_sources, 0.5, 10);
    let ncd_elapsed = start.elapsed();

    println!("\n--- NCD Detection (threshold 0.5) ---");
    println!("Time: {:.2}ms", ncd_elapsed.as_secs_f64() * 1000.0);
    println!("Candidate pairs: {}", ncd_candidates.len());
    for c in ncd_candidates.iter().take(20) {
        println!("  {:.3}  {}  <->  {}", c.distance, c.path_a, c.path_b);
    }

    // --- NCD detection (threshold 0.6) ---
    let start2 = Instant::now();
    let ncd_candidates_60 = statico::duplication::find_candidate_pairs(&file_sources, 0.6, 10);
    let ncd_elapsed2 = start2.elapsed();
    println!("\n--- NCD Detection (threshold 0.6) ---");
    println!("Time: {:.2}ms", ncd_elapsed2.as_secs_f64() * 1000.0);
    println!("Candidate pairs: {}", ncd_candidates_60.len());
    for c in ncd_candidates_60.iter().take(20) {
        println!("  {:.3}  {}  <->  {}", c.distance, c.path_a, c.path_b);
    }

    // --- Block/Fragment detection (existing) ---
    let _file_blocks: BTreeMap<String, Vec<statico::parse::blocks::CodeBlock>> = BTreeMap::new();

    // --- Analysis ---
    let ncd_pairs: HashSet<(String, String)> = ncd_candidates
        .iter()
        .map(|c| {
            let mut pair = [c.path_a.clone(), c.path_b.clone()];
            pair.sort();
            (pair[0].clone(), pair[1].clone())
        })
        .collect();

    // Group by directory for structural insight
    let mut by_dir: std::collections::HashMap<String, Vec<&statico::duplication::NcdCandidate>> =
        std::collections::HashMap::new();
    for c in &ncd_candidates {
        let dir = c.path_a.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        by_dir.entry(dir.to_string()).or_default().push(c);
    }

    println!("\n--- Cross-directory candidates (potential copy-paste) ---");
    let cross_dir: Vec<_> = ncd_candidates
        .iter()
        .filter(|c| {
            let dir_a = c.path_a.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            let dir_b = c.path_b.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            dir_a != dir_b
        })
        .collect();
    println!("{} cross-directory pairs", cross_dir.len());
    for c in cross_dir.iter().take(10) {
        println!("  {:.3}  {}  <->  {}", c.distance, c.path_a, c.path_b);
    }

    println!("\n--- Summary ---");
    println!("Files:      {}", file_sources.len());
    println!("Pairs:      {} ({} comparisons)", ncd_pairs.len(), file_sources.len() * (file_sources.len() - 1) / 2);
    println!("NCD time:   {:.2}ms (all pairs)", ncd_elapsed.as_secs_f64() * 1000.0);
}
