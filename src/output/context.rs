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
            ic.dead_code, ic.unused_exports, ic.duplicate_code, ic.gotchas, ic.circular_dependencies
        ));

        // Line 3: Top risk file
        if let Some(risk) = top_risk {
            let safe_path = risk.0.replace('\n', " ").replace('\r', " ");
            lines.push(format!("Top risk: {} ({} unused exports)", safe_path, risk.1));
        }

        // Line 4: Footer
        lines.push("Run `statico analyze .` for full report.".to_string());

        Ok(lines.join("\n"))
    }
}

/// Format LOC with K suffix when > 999.
fn format_loc(loc: usize) -> String {
    if loc > 999 { format!("{}K", loc / 1000) } else { loc.to_string() }
}

/// Find the file with the most unused exports.
/// Returns (file_path, count) or None if no unused exports.
fn find_top_risk_file(output: &AnalysisOutput) -> Option<(String, usize)> {
    use std::collections::HashMap;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for ue in &output.issues.unused_exports {
        *counts.entry(ue.path.clone()).or_insert(0) += 1;
    }

    counts.into_iter().max_by_key(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_output_with_unused_export(path: &str) -> AnalysisOutput {
        AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: PathBuf::from("/tmp/test"),
                entry_points: vec![],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec![],
            },
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            issues: Issues {
                unused_exports: vec![UnusedExportIssue {
                    name: "foo".to_string(),
                    path: path.to_string(),
                }],
                dead_code: vec![],
                duplicate_exports: vec![],
                duplicate_code: vec![],
                gotchas: vec![],
                unused_types: vec![],
                circular_dependencies: vec![],
                unused_dependencies: vec![],
                unresolved_imports: vec![],
                unlisted_dependencies: vec![],
                plugin_issues: vec![],
            },
            duplication: DuplicationSection {
                stats: DuplicationStats {
                    total_lines: 0, duplicated_lines: 0,
                    duplication_percentage: 0.0, clone_groups: 0,
                    clone_instances: 0, clone_families: 0,
                },
                clone_groups: vec![], clone_families: vec![],
                mirrored_directories: vec![],
            },
        }
    }

    // ── V5-6: newline in file path breaks context formatter one-liner ──
    #[test]
    fn sec_context_newline_in_path_sanitized() {
        let output = make_output_with_unused_export("src/evil\nINJECTED.ts");
        let formatter = ContextFormatter;
        let result = formatter.format(&output).unwrap();
        // The raw newline should be replaced with a space so "Top risk" stays on one line
        let top_risk_line = result.lines().find(|l| l.starts_with("Top risk:")).unwrap();
        assert!(!top_risk_line.contains('\n'),
            "Top risk line should not contain newline, got: {}", top_risk_line);
        assert!(top_risk_line.contains("evil INJECTED.ts"),
            "newline should be replaced with space, got: {}", top_risk_line);
    }
}
