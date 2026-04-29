//! Markdown summary report formatter.

use crate::output::{OutputFormatter, compute_summary};
use crate::types::AnalysisOutput;

/// Markdown report formatter.
pub struct MarkdownFormatter;

impl OutputFormatter for MarkdownFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let summary = compute_summary(output);
        let mut md = String::new();

        // Title
        md.push_str("# statico Analysis Report\n\n");

        // Executive Summary
        md.push_str("## Executive Summary\n\n");
        md.push_str(&format!("- **Files analyzed:** {}\n", summary.total_files));
        md.push_str(&format!("- **Total lines of code:** {}\n", summary.total_lines));
        md.push_str(&format!("- **Duplication:** {:.1}%\n", summary.duplication_percentage));
        md.push_str(&format!("- **Total exports:** {}\n", summary.total_exports));
        md.push_str(&format!("- **Entry points:** {}\n", output.structure.entry_points.len()));
        md.push_str("\n### Issue Counts\n\n");
        md.push_str("| Category | Count |\n|---|---|\n");
        md.push_str(&format!("| Dead code | {} |\n", summary.issue_counts.dead_code));
        md.push_str(&format!("| Unused exports | {} |\n", summary.issue_counts.unused_exports));
        md.push_str(&format!("| Unused types | {} |\n", summary.issue_counts.unused_types));
        md.push_str(&format!("| Duplicate code | {} |\n", summary.issue_counts.duplicate_code));
        md.push_str(&format!("| Gotchas | {} |\n", summary.issue_counts.gotchas));
        md.push_str(&format!("| Circular deps | {} |\n", summary.issue_counts.circular_dependencies));
        md.push_str(&format!("| Unused deps | {} |\n", summary.issue_counts.unused_dependencies));
        md.push_str(&format!("| Duplicate exports | {} |\n", summary.issue_counts.duplicate_exports));
        md.push_str(&format!("| Unresolved imports | {} |\n", summary.issue_counts.unresolved_imports));
        md.push_str(&format!("| Unlisted deps | {} |\n", summary.issue_counts.unlisted_dependencies));
        md.push_str("\n");

        // Health Dashboard
        md.push_str("## Health Dashboard\n\n");
        let score = summary.health_score;
        let emoji = if score >= 80.0 { "🟢" } else if score >= 50.0 { "🟡" } else { "🔴" };
        md.push_str(&format!("**Overall Health Score: {} {:.1}/100**\n\n", emoji, score));
        md.push_str(&health_bar(score));
        md.push_str("\n");

        // Dead Code
        if !output.issues.dead_code.is_empty() {
            md.push_str("## Dead Code\n\n");
            md.push_str("| File | Lines | Confidence | Reason |\n|---|---|---|---|\n");
            let mut sorted = output.issues.dead_code.clone();
            sorted.sort_by(|a, b| b.lines_of_code.cmp(&a.lines_of_code));
            for dc in sorted.iter().take(50) {
                md.push_str(&format!(
                    "| `{}` | {} | {:.0}% | {} |\n",
                    dc.path, dc.lines_of_code, dc.confidence * 100.0, dc.reason
                ));
            }
            md.push_str("\n");
        }

        // Unused Exports
        if !output.issues.unused_exports.is_empty() {
            md.push_str("## Unused Exports (Top 20)\n\n");
            md.push_str("| Export | File |\n|---|---|\n");
            for ue in output.issues.unused_exports.iter().take(20) {
                md.push_str(&format!("| `{}` | `{}` |\n", ue.name, ue.path));
            }
            md.push_str("\n");
        }

        // Unused Types
        if !output.issues.unused_types.is_empty() {
            md.push_str("## Unused Types (Top 20)\n\n");
            md.push_str("| Type | Kind | File |\n|---|---|---|\n");
            for ut in output.issues.unused_types.iter().take(20) {
                md.push_str(&format!("| `{}` | {} | `{}` |\n", ut.name, ut.kind, ut.path));
            }
            md.push_str("\n");
        }

        // Duplication
        if !output.duplication.clone_groups.is_empty() {
            md.push_str("## Duplication\n\n");
            md.push_str(&format!(
                "**Stats:** {} clone groups, {} duplicated lines ({:.1}%)\n\n",
                output.duplication.stats.clone_groups,
                output.duplication.stats.duplicated_lines,
                output.duplication.stats.duplication_percentage,
            ));
            md.push_str("### Top 10 Clone Groups\n\n");
            md.push_str("| # | Files | Lines |\n|---|---|---|\n");
            let mut groups = output.duplication.clone_groups.clone();
            groups.sort_by(|a, b| b.line_count.cmp(&a.line_count));
            for (i, g) in groups.iter().take(10).enumerate() {
                let files: Vec<String> = g.instances.iter()
                    .map(|inst| format!("{}:L{}", inst.file, inst.start_line))
                    .collect();
                md.push_str(&format!("| {} | {} | {} |\n", i + 1, files.join(", "), g.line_count));
            }
            md.push_str("\n");

            if !output.duplication.clone_families.is_empty() {
                md.push_str("### Clone Families\n\n");
                for fam in &output.duplication.clone_families {
                    md.push_str(&format!(
                        "- **{} groups, {} lines**: {}\n",
                        fam.group_count, fam.total_duplicated_lines, fam.files.join(", ")
                    ));
                }
                md.push_str("\n");
            }
        }

        // Circular Dependencies
        if !output.issues.circular_dependencies.is_empty() {
            md.push_str("## Circular Dependencies\n\n");
            for cd in &output.issues.circular_dependencies {
                md.push_str(&format!("- {} → {}\n", cd.files.join(" → "), cd.files[0]));
            }
            md.push_str("\n");
        }

        // Framework Info
        md.push_str("## Framework Info\n\n");
        md.push_str(&format!("- **Entry points:** {}\n", output.structure.entry_points.len()));
        md.push_str(&format!("- **Config files:** {}\n", output.structure.config_files.join(", ")));

        Ok(md)
    }
}

fn health_bar(score: f64) -> String {
    let filled = (score / 5.0).round() as usize;
    let empty = 20 - filled;
    format!("`[{}{}]`", "#".repeat(filled), "-".repeat(empty))
}
