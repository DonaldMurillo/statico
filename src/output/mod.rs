//! Output formatting system for statico.
//!
//! All formatters implement the `OutputFormatter` trait and produce a string
//! representation of an `AnalysisOutput` in various formats.

pub mod diff;
pub mod html;
pub mod json_enriched;
pub mod markdown;
pub mod sarif;

use crate::types::AnalysisOutput;

/// Trait for all output formatters.
pub trait OutputFormatter {
    /// Format the analysis output into a string.
    fn format(&self, output: &AnalysisOutput) -> Result<String, String>;
}

/// Compute the summary from an AnalysisOutput.
pub fn compute_summary(output: &AnalysisOutput) -> crate::types::Summary {
    let total_files = output.structure.source_files.len();
    let total_lines: usize = output.quality.files.iter()
        .filter_map(|f| f.metrics.as_ref())
        .map(|m| m.lines_of_code)
        .sum();
    let total_exports: usize = output.quality.files.iter()
        .map(|f| f.exports.len())
        .sum();
    let total_types: usize = output.issues.unused_types.len();

    let issue_counts = crate::types::IssueCounts {
        dead_code: output.issues.dead_code.len(),
        unused_exports: output.issues.unused_exports.len(),
        unused_types: output.issues.unused_types.len(),
        duplicate_code: output.issues.duplicate_code.len(),
        gotchas: output.issues.gotchas.len(),
        circular_dependencies: output.issues.circular_dependencies.len(),
        unused_dependencies: output.issues.unused_dependencies.len(),
        duplicate_exports: output.issues.duplicate_exports.len(),
        unresolved_imports: output.issues.unresolved_imports.len(),
        unlisted_dependencies: output.issues.unlisted_dependencies.len(),
    };

    let dup_pct = output.duplication.stats.duplication_percentage;

    // Health score: start at 100, penalize for issue density.
    let total_issues = issue_counts.dead_code
        + issue_counts.unused_exports
        + issue_counts.unused_types
        + issue_counts.gotchas
        + issue_counts.circular_dependencies;
    let density = if total_files > 0 {
        total_issues as f64 / total_files as f64
    } else {
        0.0
    };
    let health_score = (100.0 - density * 10.0 - dup_pct * 0.3).max(0.0).min(100.0);

    crate::types::Summary {
        total_files,
        total_lines,
        total_exports,
        total_types,
        issue_counts,
        health_score: (health_score * 10.0).round() / 10.0,
        duplication_percentage: dup_pct,
    }
}

/// Detect framework names from the project structure.
pub fn detect_framework_names(output: &AnalysisOutput) -> Vec<String> {
    let root = &output.structure.root;
    let profiles = crate::frameworks::detect_profiles(root);
    profiles.iter()
        .filter(|p| p.name != "generic")
        .map(|p| p.name.to_string())
        .collect()
}

/// Filter issues by minimum confidence threshold.
/// Returns a modified AnalysisOutput with low-confidence issues removed.
pub fn filter_by_confidence(output: &AnalysisOutput, min_confidence: f64) -> AnalysisOutput {
    let mut filtered = output.clone();

    // We need Serialize+Deserialize for cloning. Since we can't add Clone to
    // everything, we'll filter in-place conceptually. Actually let's just filter
    // the confidence-based issue lists.
    // We need Clone on AnalysisOutput for this to work.
    filtered.issues.dead_code.retain(|i| i.confidence >= min_confidence);
    filtered.issues.duplicate_code.retain(|i| i.confidence >= min_confidence);
    filtered.issues.gotchas.retain(|i| i.confidence >= min_confidence);
    filtered
}
