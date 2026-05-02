//! Integration tests for output formatters and AI-friendly features.

use statico::output::OutputFormatter;
use std::path::Path;

/// Helper: run analyze on a fixture project.
fn analyze_fixture(name: &str) -> statico::types::AnalysisOutput {
    let root = Path::new("fixtures").join(name);
    statico::analyzer::analyze(&root).expect("analyze should succeed")
}

#[test]
fn test_sarif_output_structure() {
    let output = analyze_fixture("dead-code-project");
    let formatter = statico::output::sarif::SarifFormatter;
    let result = formatter.format(&output).expect("SARIF format should succeed");
    let sarif: serde_json::Value = serde_json::from_str(&result).expect("SARIF should be valid JSON");

    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["$schema"].is_string());
    assert!(sarif["runs"].is_array());
    assert!(sarif["runs"][0]["tool"]["driver"]["name"] == "statico");
    assert!(sarif["runs"][0]["results"].is_array());
}

#[test]
fn test_markdown_output_contains_sections() {
    let output = analyze_fixture("dead-code-project");
    let formatter = statico::output::markdown::MarkdownFormatter;
    let result = formatter.format(&output).expect("Markdown format should succeed");

    assert!(result.contains("# "));
    assert!(result.contains("Summary") || result.contains("summary"));
}

#[test]
fn test_html_output_is_valid() {
    let output = analyze_fixture("dead-code-project");
    let formatter = statico::output::html::HtmlFormatter;
    let result = formatter.format(&output).expect("HTML format should succeed");

    assert!(result.contains("<!DOCTYPE html>") || result.contains("<html"));
    assert!(result.contains("</html>"));
}

#[test]
fn test_enriched_json_has_schema() {
    let output = analyze_fixture("dead-code-project");
    let formatter = statico::output::json_enriched::EnrichedJsonFormatter;
    let result = formatter.format(&output).expect("Enriched JSON should succeed");
    let json: serde_json::Value = serde_json::from_str(&result).expect("Should be valid JSON");

    assert_eq!(json["version"], "0.2.0");
    assert!(json["$schema"].is_string());
    assert!(json["summary"].is_object());
    assert!(json["summary"]["total_files"].is_number());
    assert!(json["summary"]["health_score"].is_number());
}

#[test]
fn test_enriched_json_health_score_range() {
    let output = analyze_fixture("dead-code-project");
    let formatter = statico::output::json_enriched::EnrichedJsonFormatter;
    let result = formatter.format(&output).expect("Enriched JSON should succeed");
    let json: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");

    let score = json["summary"]["health_score"].as_f64().unwrap();
    assert!((0.0..=100.0).contains(&score), "health_score should be 0-100, got {}", score);
}

#[test]
fn test_diff_computes_correctly() {
    let before = analyze_fixture("dead-code-project");
    let after = analyze_fixture("dead-code-project");
    let diff = statico::output::diff::compute_diff(&before, &after);

    // Same project → no new or fixed issues.
    assert!(diff.new_issues.is_empty(), "same project should have no new issues");
    assert!(diff.fixed_issues.is_empty(), "same project should have no fixed issues");
}

#[test]
fn test_diff_json_output() {
    let before = analyze_fixture("dead-code-project");
    let after = analyze_fixture("dead-code-project");
    let diff = statico::output::diff::compute_diff(&before, &after);

    let result = statico::output::diff::format_diff_json(&diff).expect("diff JSON should succeed");
    let json: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");
    assert!(json["new_issues"].is_array());
    assert!(json["fixed_issues"].is_array());
    assert!(json["persisting"].is_array());
}

#[test]
fn test_confidence_filter() {
    let output = analyze_fixture("dead-code-project");
    let filtered = statico::output::filter_by_confidence(&output, 0.9);

    // All remaining dead code should have confidence >= 0.9.
    for item in &filtered.issues.dead_code {
        assert!(item.confidence >= 0.9, "filtered dead code confidence should be >= 0.9");
    }
}

#[test]
fn test_sarif_has_rules() {
    let output = analyze_fixture("nextjs-project");
    let formatter = statico::output::sarif::SarifFormatter;
    let result = formatter.format(&output).expect("SARIF should succeed");
    let sarif: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");

    let rules = &sarif["runs"][0]["tool"]["driver"]["rules"];
    assert!(rules.is_array(), "SARIF should include rules array");
}
