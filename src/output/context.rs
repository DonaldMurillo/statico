//! Ultra-compact one-liner formatter for system prompts / AGENTS.md injection.
//!
//! Produces ~100 tokens of plain text summarizing code health.

use crate::output::{OutputFormatter, compute_summary};
use crate::types::AnalysisOutput;

/// Ultra-compact one-liner formatter (`--format context`).
pub struct ContextFormatter;

impl OutputFormatter for ContextFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let summary = compute_summary(output);

        let loc_str = format_loc(summary.total_lines);
        let total_exports = summary.total_exports;
        let ic = &summary.issue_counts;

        // Find top risk file: the file with the most unused exports
        let top_risk = find_top_risk_file(output);

        let mut lines: Vec<String> = Vec::new();

        // Line 1: Health + scale
        lines.push(format!(
            "Statico Code Health: {:.1}/100 | {} files | {} LOC | {} exports",
            summary.health_score, summary.total_files, loc_str, total_exports
        ));

        // Line 2: Issue counts
        lines.push(format!(
            "Issues: {} dead, {} unused exports, {} duplication, {} gotchas, {} circular deps",
            ic.dead_code,
            ic.unused_exports,
            ic.duplicate_code,
            ic.gotchas,
            ic.circular_dependencies
        ));

        // Line 3: Top risk file
        if let Some(risk) = top_risk {
            lines.push(format!(
                "Top risk: {} ({} unused exports)",
                risk.0, risk.1
            ));
        }

        // Line 4: Footer
        lines.push("Run `statico analyze .` for full report.".to_string());

        Ok(lines.join("\n"))
    }
}

/// Format LOC with K suffix when > 999.
fn format_loc(loc: usize) -> String {
    if loc > 999 {
        format!("{}K", loc / 1000)
    } else {
        loc.to_string()
    }
}

/// Find the file with the most unused exports.
/// Returns (file_path, count) or None if no unused exports.
fn find_top_risk_file(output: &AnalysisOutput) -> Option<(String, usize)> {
    use std::collections::HashMap;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for ue in &output.issues.unused_exports {
        *counts.entry(ue.path.clone()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(path, count)| (path, count))
}
