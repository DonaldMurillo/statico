//! Property-based and fuzz tests for statico invariants.
//!
//! Uses real fixture projects and proptest to verify structural invariants
//! that must hold for *any* valid AnalysisOutput.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name).leak()
}

/// Run analyze on a fixture and unwrap the result.
fn analyze_fixture(name: &str) -> statico::types::AnalysisOutput {
    statico::analyzer::analyze(fixture(name)).expect("analyze should succeed")
}

const FIXTURES: &[&str] = &[
    "minimal-ts-project",
    "dead-code-project",
    "duplicate-exports-project",
    "empty-project",
    "malformed-project",
    "nextjs-project",
    "payload-project",
];

// ---------------------------------------------------------------------------
// a) Dead code files are a subset of source files
// ---------------------------------------------------------------------------

#[test]
fn prop_dead_code_subset_of_source_files() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        let source_paths: BTreeSet<&str> = out.structure.source_files.iter().map(|sf| sf.path.as_str()).collect();
        for dc in &out.issues.dead_code {
            assert!(
                source_paths.contains(dc.path.as_str()),
                "dead_code file '{}' not in source_files ({})",
                dc.path,
                name,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// b) Unused exports paths are a subset of source files
// ---------------------------------------------------------------------------

#[test]
fn prop_unused_exports_subset_of_source_files() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        let source_paths: BTreeSet<&str> = out.structure.source_files.iter().map(|sf| sf.path.as_str()).collect();
        for ue in &out.issues.unused_exports {
            assert!(
                source_paths.contains(ue.path.as_str()),
                "unused_exports path '{}' not in source_files ({})",
                ue.path,
                name,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// c) Duplication percentage is in [0, 100]
// ---------------------------------------------------------------------------

#[test]
fn prop_duplication_percentage_in_range() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        let pct = out.duplication.stats.duplication_percentage;
        assert!((0.0..=100.0).contains(&pct), "duplication_percentage = {} not in [0, 100] ({})", pct, name,);
    }
}

// ---------------------------------------------------------------------------
// d) Duplicated lines <= total lines
// ---------------------------------------------------------------------------

#[test]
fn prop_duplicated_lines_le_total_lines() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        assert!(
            out.duplication.stats.duplicated_lines <= out.duplication.stats.total_lines,
            "duplicated_lines ({}) > total_lines ({}) ({})",
            out.duplication.stats.duplicated_lines,
            out.duplication.stats.total_lines,
            name,
        );
    }
}

// ---------------------------------------------------------------------------
// e) Clone group instances have valid line ranges
// ---------------------------------------------------------------------------

#[test]
fn prop_clone_instances_valid_line_ranges() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        for (gi, group) in out.duplication.clone_groups.iter().enumerate() {
            for (ii, inst) in group.instances.iter().enumerate() {
                assert!(
                    inst.start_line >= 1,
                    "clone_group[{}].instances[{}].start_line = {} < 1 ({})",
                    gi,
                    ii,
                    inst.start_line,
                    name,
                );
                assert!(
                    inst.end_line >= inst.start_line,
                    "clone_group[{}].instances[{}].end_line {} < start_line {} ({})",
                    gi,
                    ii,
                    inst.end_line,
                    inst.start_line,
                    name,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// f) Confidence scores in [0, 1]
// ---------------------------------------------------------------------------

#[test]
fn prop_confidence_scores_in_range() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);

        for dc in &out.issues.dead_code {
            assert!(
                (0.0..=1.0).contains(&dc.confidence),
                "dead_code confidence {} not in [0,1] ({})",
                dc.confidence,
                name,
            );
        }
        for dup in &out.issues.duplicate_code {
            assert!(
                (0.0..=1.0).contains(&dup.confidence),
                "duplicate_code confidence {} not in [0,1] ({})",
                dup.confidence,
                name,
            );
        }
        for g in &out.issues.gotchas {
            assert!((0.0..=1.0).contains(&g.confidence), "gotcha confidence {} not in [0,1] ({})", g.confidence, name,);
        }
    }
}

// ---------------------------------------------------------------------------
// g) Fuzz the parser — random strings don't panic
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fuzz_extract_imports_no_panic(s in ".*") {
        let mut parser = statico::parse::AstParser::new().expect("parser init");
        if let Some(result) = parser.parse(&s, false) {
            let root = result.tree.root_node();
            let _ = statico::parse::imports::extract_imports(root, &s);
        }
    }

    #[test]
    fn fuzz_extract_exports_no_panic(s in ".*") {
        let mut parser = statico::parse::AstParser::new().expect("parser init");
        if let Some(result) = parser.parse(&s, false) {
            let root = result.tree.root_node();
            let _ = statico::parse::exports::extract_exports(root, &s);
        }
    }

    #[test]
    fn fuzz_count_loc_no_panic(s in ".*") {
        let (loc, total) = statico::parse::metrics::count_loc(&s);
        assert!(loc <= total, "loc ({}) should be <= total ({})", loc, total);
    }
}

// ---------------------------------------------------------------------------
// h) Circular deps are actually cycles
// ---------------------------------------------------------------------------

#[test]
fn prop_circular_deps_form_cycles() {
    for &name in FIXTURES {
        let out = analyze_fixture(name);
        if out.issues.circular_dependencies.is_empty() {
            continue;
        }

        // Build adjacency list from dependency graph.
        let mut adj: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for fi in &out.dependencies.imports {
            let targets: BTreeSet<&str> = fi.targets.iter().map(|t| t.as_str()).collect();
            adj.insert(fi.source.as_str(), targets);
        }

        for (ci, cycle) in out.issues.circular_dependencies.iter().enumerate() {
            let files = &cycle.files;
            assert!(files.len() >= 2, "circular_dep[{}] has {} files, need >= 2 ({})", ci, files.len(), name,);

            // Verify each consecutive pair has an edge in the dependency graph.
            for i in 0..files.len() {
                let from = files[i].as_str();
                let to = files[(i + 1) % files.len()].as_str();
                let has_edge = adj.get(from).is_some_and(|targets| targets.contains(to));
                assert!(has_edge, "circular_dep[{}]: no edge '{}' -> '{}' ({})", ci, from, to, name,);
            }
        }
    }
}
