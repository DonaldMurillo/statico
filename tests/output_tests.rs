//! Integration-level tests for output formatters and core functions.

use statico::output::OutputFormatter;
use statico::types::*;
use std::path::PathBuf;

/// Helper to create a minimal but valid AnalysisOutput for testing.
pub fn minimal_output() -> AnalysisOutput {
    AnalysisOutput {
        version: None,
        summary: None,
        detected_frameworks: None,
        monorepo: None,
        structure: Structure {
            root: PathBuf::from("/test/project"),
            entry_points: vec!["src/index.ts".into()],
            implicit_entries: vec![],
            source_files: vec![SourceFile { path: "src/index.ts".into(), language: "typescript".into() }],
            config_files: vec![],
        },
        dependencies: Dependencies { imports: vec![], external: vec![] },
        quality: Quality {
            files: vec![FileQuality {
                path: "src/index.ts".into(),
                metrics: Some(Metrics {
                    lines_of_code: 10,
                    total_lines: 15,
                    functions: 1,
                    classes: 0,
                    complexity: 1,
                    max_nesting_depth: 0,
                }),
                exports: vec!["main".into()],
                parse_errors: vec![],
            }],
        },
        issues: Issues {
            dead_code: vec![],
            unused_exports: vec![],
            duplicate_exports: vec![],
            duplicate_code: vec![],
            gotchas: vec![],
            unused_types: vec![],
            circular_dependencies: vec![],
            unused_dependencies: vec![],
            unresolved_imports: vec![],
            unlisted_dependencies: vec![],
        },
        duplication: DuplicationSection {
            stats: DuplicationStats {
                total_lines: 100,
                duplicated_lines: 5,
                duplication_percentage: 5.0,
                clone_groups: 0,
                clone_instances: 0,
                clone_families: 0,
            },
            clone_groups: vec![],
            clone_families: vec![],
            mirrored_directories: vec![],
        },
    }
}

/// Helper to create an output with some issues populated.
pub fn output_with_issues() -> AnalysisOutput {
    let mut output = minimal_output();
    output.issues.dead_code.push(DeadCodeIssue {
        path: "src/dead.ts".into(),
        lines_of_code: 50,
        confidence: 0.9,
        reason: "Not reachable from entry points".into(),
    });
    output.issues.unused_exports.push(UnusedExportIssue {
        name: "unusedFn".into(),
        path: "src/utils.ts".into(),
    });
    output.issues.unused_types.push(UnusedTypeIssue {
        name: "OldType".into(),
        path: "src/types.ts".into(),
        kind: "interface".into(),
    });
    output.issues.circular_dependencies.push(CircularDepIssue {
        files: vec!["src/a.ts".into(), "src/b.ts".into(), "src/a.ts".into()],
    });
    output.issues.gotchas.push(GotchaIssue {
        file: "src/app.ts".into(),
        line: 42,
        rule: "any-cast".into(),
        severity: "warning".into(),
        message: "Avoid using `as any` cast".into(),
        confidence: 0.85,
        snippet: "const x = data as any;".into(),
    });
    output.issues.duplicate_exports.push(DuplicateExportIssue {
        name: "VERSION".into(),
        locations: vec!["src/a.ts".into(), "src/b.ts".into()],
    });
    output.issues.unresolved_imports.push(UnresolvedImportIssue {
        source_file: "src/app.ts".into(),
        import_spec: "./missing".into(),
    });
    output.issues.unused_dependencies.push(UnusedDepIssue {
        package_name: "lodash".into(),
        location: "package.json".into(),
    });
    output.issues.unlisted_dependencies.push(UnlistedDepIssue {
        package_name: "unknown-pkg".into(),
        imported_by: "src/app.ts".into(),
    });
    output
}

// ---------------------------------------------------------------------------
// output/mod.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_compute_summary_empty_output() {
    let output = minimal_output();
    let summary = statico::output::compute_summary(&output);
    assert_eq!(summary.total_files, 1);
    assert_eq!(summary.total_lines, 10);
    assert_eq!(summary.total_exports, 1);
    // health = 100 - 0*density - 5.0*0.3 = 98.5 (dup penalty)
    assert_eq!(summary.health_score, 98.5);
    assert_eq!(summary.duplication_percentage, 5.0);
    assert_eq!(summary.issue_counts.dead_code, 0);
    assert_eq!(summary.issue_counts.unused_exports, 0);
}

#[test]
fn test_compute_summary_with_issues() {
    let output = output_with_issues();
    let summary = statico::output::compute_summary(&output);
    assert_eq!(summary.total_files, 1);
    assert_eq!(summary.issue_counts.dead_code, 1);
    assert_eq!(summary.issue_counts.unused_exports, 1);
    assert_eq!(summary.issue_counts.gotchas, 1);
    assert_eq!(summary.issue_counts.circular_dependencies, 1);
    // health should be < 100 since there are issues
    assert!(summary.health_score < 100.0);
}

#[test]
fn test_compute_summary_health_score_clamped() {
    let mut output = minimal_output();
    // Add tons of issues to drive score negative — should clamp to 0
    for _ in 0..50 {
        output.issues.dead_code.push(DeadCodeIssue {
            path: "x.ts".into(),
            lines_of_code: 10,
            confidence: 0.9,
            reason: "test".into(),
        });
    }
    let summary = statico::output::compute_summary(&output);
    assert!(summary.health_score >= 0.0);
    assert!(summary.health_score <= 100.0);
}

#[test]
fn test_filter_by_confidence() {
    let mut output = output_with_issues();
    // Add a low-confidence dead code issue
    output.issues.dead_code.push(DeadCodeIssue {
        path: "src/low.ts".into(),
        lines_of_code: 5,
        confidence: 0.3,
        reason: "maybe dead".into(),
    });
    assert_eq!(output.issues.dead_code.len(), 2);

    let filtered = statico::output::filter_by_confidence(&output, 0.5);
    assert_eq!(filtered.issues.dead_code.len(), 1);
    assert_eq!(filtered.issues.dead_code[0].path, "src/dead.ts");
}

// ---------------------------------------------------------------------------
// output/markdown.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_formatter_empty() {
    let output = minimal_output();
    let result = statico::output::markdown::MarkdownFormatter.format(&output).unwrap();
    assert!(result.contains("# statico Analysis Report"));
    assert!(result.contains("Files analyzed"));
    assert!(result.contains("Health Score"));
}

#[test]
fn test_markdown_formatter_with_issues() {
    let output = output_with_issues();
    let result = statico::output::markdown::MarkdownFormatter.format(&output).unwrap();
    assert!(result.contains("Dead code"));
    assert!(result.contains("| 1 |")); // at least 1 dead code
    assert!(result.contains("Circular deps"));
}

// ---------------------------------------------------------------------------
// output/sarif.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_sarif_formatter_empty() {
    let output = minimal_output();
    let result = statico::output::sarif::SarifFormatter.format(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["$schema"], "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json");
    assert_eq!(parsed["version"], "2.1.0");
    let runs = parsed["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    // No issues = no results
    let results = runs[0]["results"].as_array().unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_sarif_formatter_with_issues() {
    let output = output_with_issues();
    let result = statico::output::sarif::SarifFormatter.format(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let results = parsed["runs"][0]["results"].as_array().unwrap();
    assert!(!results.is_empty());
    // Check a dead_code result exists
    let has_dead_code = results.iter().any(|r| r["ruleId"] == "dead_code");
    assert!(has_dead_code);
    // Check a gotcha result exists
    let has_gotcha = results.iter().any(|r| r["ruleId"] == "gotcha");
    assert!(has_gotcha);
}

// ---------------------------------------------------------------------------
// output/json_enriched.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_enriched_json_formatter() {
    let output = minimal_output();
    let result = statico::output::json_enriched::EnrichedJsonFormatter.format(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.get("$schema").is_some());
    assert_eq!(parsed["version"], "0.2.0");
    assert!(parsed.get("summary").is_some());
    assert!(parsed.get("structure").is_some());
}

// ---------------------------------------------------------------------------
// output/ai.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_ai_formatter() {
    let output = output_with_issues();
    let result = statico::output::ai::AiFormatter.format(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.get("schema").is_some());
    assert!(parsed.get("summary").is_some());
    assert!(parsed.get("top_issues").is_some());
}

// ---------------------------------------------------------------------------
// output/diff.rs tests
// ---------------------------------------------------------------------------

#[test]
fn test_diff_empty_to_issues() {
    let before = minimal_output();
    let after = output_with_issues();
    let diff = statico::output::diff::compute_diff(&before, &after);
    assert!(!diff.new_issues.is_empty());
    assert!(diff.fixed_issues.is_empty());
    assert!(diff.persisting.is_empty());
}

#[test]
fn test_diff_issues_to_empty() {
    let before = output_with_issues();
    let after = minimal_output();
    let diff = statico::output::diff::compute_diff(&before, &after);
    assert!(diff.new_issues.is_empty());
    assert!(!diff.fixed_issues.is_empty());
    assert!(diff.persisting.is_empty());
}

#[test]
fn test_diff_identical() {
    let output = output_with_issues();
    let diff = statico::output::diff::compute_diff(&output, &output);
    assert!(diff.new_issues.is_empty());
    assert!(diff.fixed_issues.is_empty());
    assert!(!diff.persisting.is_empty());
}

#[test]
fn test_diff_json_format() {
    let before = minimal_output();
    let after = output_with_issues();
    let diff = statico::output::diff::compute_diff(&before, &after);
    let json = statico::output::diff::format_diff_json(&diff).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("new_issues").is_some());
    assert!(parsed.get("fixed_issues").is_some());
}

#[test]
fn test_diff_markdown_format() {
    let before = minimal_output();
    let after = output_with_issues();
    let diff = statico::output::diff::compute_diff(&before, &after);
    let md = statico::output::diff::format_diff_markdown(&diff).unwrap();
    assert!(md.contains("# statico Diff Report"));
    assert!(md.contains("New Issues"));
}

// ---------------------------------------------------------------------------
// types.rs serialization round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_analysis_output_serialization_roundtrip() {
    let output = output_with_issues();
    let json = serde_json::to_string(&output).unwrap();
    let deserialized: AnalysisOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.issues.dead_code.len(), output.issues.dead_code.len());
    assert_eq!(deserialized.issues.unused_exports.len(), output.issues.unused_exports.len());
    assert_eq!(deserialized.issues.gotchas.len(), output.issues.gotchas.len());
}

#[test]
fn test_minimal_output_json_keys() {
    let output = minimal_output();
    let json = serde_json::to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Optional fields should be skipped
    assert!(parsed.get("version").is_none());
    assert!(parsed.get("summary").is_none());
    assert!(parsed.get("monorepo").is_none());
    // Required fields should be present
    assert!(parsed.get("structure").is_some());
    assert!(parsed.get("dependencies").is_some());
    assert!(parsed.get("quality").is_some());
    assert!(parsed.get("issues").is_some());
    assert!(parsed.get("duplication").is_some());
}
