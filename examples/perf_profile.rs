use std::time::Instant;

fn main() {
    let root = std::path::Path::new("benchmarks/repos/calcom");

    // Phase 1: Discovery
    let t0 = Instant::now();
    let source_files = statico::discovery::discover_source_files(root).unwrap();
    let t_discovery = t0.elapsed();
    println!("Discovery: {:.0}ms ({} files)", t_discovery.as_millis(), source_files.len());

    // Phase 2: Full analysis
    let t1 = Instant::now();
    let result = statico::analyzer::analyze(root).unwrap();
    let t_analyze = t1.elapsed();
    println!("Full analysis: {:.0}ms", t_analyze.as_millis());
    println!("  Dead code: {}", result.issues.dead_code.len());
    println!("  Unused exports: {}", result.issues.unused_exports.len());
    println!("  Source files: {}", result.structure.source_files.len());
}
