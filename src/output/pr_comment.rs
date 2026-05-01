//! GitHub-flavored Markdown PR comment formatter.
//!
//! Produces a rich comment with emoji indicators, tables, and actionable items
//! suitable for posting as a PR review comment.

use crate::output::{OutputFormatter, compute_summary};
use crate::types::AnalysisOutput;

/// GitHub-flavored Markdown PR comment formatter (`--format pr-comment`).
pub struct PrCommentFormatter;

/// Escape special characters for safe embedding in Markdown table cells.
fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|")
     .replace('[', "\\[")
     .replace(']', "\\]")
     .replace('`', "\\`")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('\n', " ")
     .replace('\r', " ")
}

impl OutputFormatter for PrCommentFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let summary = compute_summary(output);
        let mut md = String::new();

        // Header
        md.push_str("## 🔍 Statico Analysis\n\n");

        // Summary table
        let score = summary.health_score;
        let emoji = if score >= 80.0 {
            "🟢"
        } else if score >= 60.0 {
            "🟡"
        } else {
            "🔴"
        };

        md.push_str("| Metric | Value |\n|---|---|\n");
        md.push_str(&format!("| Health Score | {} {:.1}/100 |\n", emoji, score));
        md.push_str(&format!("| Files analyzed | {} |\n", summary.total_files));
        md.push_str(&format!("| Lines of code | {} |\n", summary.total_lines));
        md.push_str(&format!("| Duplication | {:.1}% |\n", summary.duplication_percentage));
        md.push_str(&format!("| Total exports | {} |\n", summary.total_exports));
        md.push('\n');

        // Issue breakdown table
        let ic = &summary.issue_counts;
        let total_issues = ic.dead_code
            + ic.unused_exports
            + ic.unused_types
            + ic.duplicate_code
            + ic.gotchas
            + ic.circular_dependencies;

        md.push_str("### Issue Breakdown\n\n");
        md.push_str("| Category | Count |\n|---|---|\n");
        if ic.dead_code > 0 {
            md.push_str(&format!("| ☠️ Dead code | {} |\n", ic.dead_code));
        }
        if ic.unused_exports > 0 {
            md.push_str(&format!("| 📦 Unused exports | {} |\n", ic.unused_exports));
        }
        if ic.unused_types > 0 {
            md.push_str(&format!("| 📝 Unused types | {} |\n", ic.unused_types));
        }
        if ic.duplicate_code > 0 {
            md.push_str(&format!("| 📋 Duplicate code | {} |\n", ic.duplicate_code));
        }
        if ic.gotchas > 0 {
            md.push_str(&format!("| ⚠️ Gotchas | {} |\n", ic.gotchas));
        }
        if ic.circular_dependencies > 0 {
            md.push_str(&format!("| 🔄 Circular deps | {} |\n", ic.circular_dependencies));
        }
        if total_issues == 0 {
            md.push_str("| ✅ No issues found | 0 |\n");
        }
        md.push('\n');

        // Top 5 most impactful issues
        let top_issues = build_impactful_issues(output);
        if !top_issues.is_empty() {
            md.push_str("### Top Issues\n\n");
            md.push_str("| # | Category | File | Impact | Details |\n");
            md.push_str("|---|---|---|---|---|\n");
            for (i, issue) in top_issues.iter().enumerate() {
                md.push_str(&format!(
                    "| {} | {} | `{}` | {} | {} |\n",
                    i + 1,
                    escape_md_cell(&issue.category),
                    escape_md_cell(&issue.file),
                    escape_md_cell(&issue.impact),
                    escape_md_cell(&issue.details)
                ));
            }
            md.push('\n');
        }

        // Circular dependencies section
        if !output.issues.circular_dependencies.is_empty() {
            md.push_str("### 🔄 Circular Dependencies\n\n");
            for cd in &output.issues.circular_dependencies {
                let chain: Vec<String> = cd.files.iter().map(|f| format!("`{}`", escape_md_cell(f))).collect();
                md.push_str(&format!("- {} → `{}`\n", chain.join(" → "), escape_md_cell(&cd.files[0])));
            }
            md.push('\n');
        }

        // Dead code section
        if !output.issues.dead_code.is_empty() {
            md.push_str("### ☠️ Dead Code\n\n");
            md.push_str("| File | LOC | Confidence | Reason |\n");
            md.push_str("|---|---|---|---|\n");
            let mut sorted = output.issues.dead_code.clone();
            sorted.sort_by(|a, b| b.lines_of_code.cmp(&a.lines_of_code));
            for dc in sorted.iter().take(10) {
                md.push_str(&format!(
                    "| `{}` | {} | {:.0}% | {} |\n",
                    escape_md_cell(&dc.path),
                    dc.lines_of_code,
                    dc.confidence * 100.0,
                    escape_md_cell(&dc.reason)
                ));
            }
            md.push('\n');
        }

        // Footer
        md.push_str("---\n");
        md.push_str("Generated by [statico](https://github.com/domvrt/statico) code health analyzer\n");

        Ok(md)
    }
}

/// A ranked issue for the PR comment.
struct RankedIssue {
    category: String,
    file: String,
    impact: String,
    details: String,
}

fn build_impactful_issues(output: &AnalysisOutput) -> Vec<RankedIssue> {
    let mut issues: Vec<RankedIssue> = Vec::new();

    // Dead code — impact is LOC wasted
    let mut dead = output.issues.dead_code.clone();
    dead.sort_by(|a, b| b.lines_of_code.cmp(&a.lines_of_code));
    for dc in dead.iter().take(3) {
        issues.push(RankedIssue {
            category: "☠️ dead_code".to_string(),
            file: dc.path.clone(),
            impact: format!("{} LOC", dc.lines_of_code),
            details: dc.reason.clone(),
        });
    }

    // Unused exports — aggregate per file, pick top files
    let file_counts = count_unused_exports_per_file(output);
    for (file, count) in file_counts.into_iter().take(3) {
        issues.push(RankedIssue {
            category: "📦 unused_exports".to_string(),
            file,
            impact: count.to_string(),
            details: format!("{} unused exports in this file", count),
        });
    }

    // Gotchas — top by confidence
    let mut gotchas = output.issues.gotchas.clone();
    gotchas.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    for g in gotchas.iter().take(2) {
        issues.push(RankedIssue {
            category: format!("⚠️ {}", g.rule),
            file: g.file.clone(),
            impact: format!("{:.0}%", g.confidence * 100.0),
            details: g.message.clone(),
        });
    }

    // Circular deps
    for cd in output.issues.circular_dependencies.iter().take(2) {
        issues.push(RankedIssue {
            category: "🔄 circular_dep".to_string(),
            file: cd.files.first().cloned().unwrap_or_default(),
            impact: format!("{} files", cd.files.len()),
            details: cd.files.join(" → "),
        });
    }

    // Sort by rough impact and take top 5
    // We don't have a single numeric impact across categories, so keep insertion order
    issues.truncate(5);
    issues
}

/// Count unused exports per file, sorted by count descending.
fn count_unused_exports_per_file(output: &AnalysisOutput) -> Vec<(String, usize)> {
    use std::collections::HashMap;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for ue in &output.issues.unused_exports {
        *counts.entry(ue.path.clone()).or_insert(0) += 1;
    }

    let mut entries: Vec<(String, usize)> = counts.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_evil_output() -> AnalysisOutput {
        AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: PathBuf::from("/project"),
                entry_points: vec![],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec![],
            },
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            issues: Issues {
                dead_code: vec![DeadCodeIssue {
                    path: "src/evil | [click](https://evil.com) | ".to_string(),
                    lines_of_code: 100,
                    confidence: 0.95,
                    reason: "Not reachable | [inject](https://evil.com)".to_string(),
                }],
                unused_exports: vec![UnusedExportIssue {
                    name: "EvilExport\n\n# Headline".to_string(),
                    path: "src/x | [evil](https://evil.com)".to_string(),
                }],
                duplicate_exports: vec![],
                duplicate_code: vec![],
                gotchas: vec![],
                unused_types: vec![],
                circular_dependencies: vec![CircularDepIssue {
                    files: vec!["a | [evil](https://x)".to_string(), "b\n\n# inject".to_string()],
                }],
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

    #[test]
    fn sec_pr_comment_escapes_pipe_in_tables() {
        let output = make_evil_output();
        let formatter = PrCommentFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("evil | [click](https://evil.com)"),
            "PR comment should escape pipe chars: {}", md);
    }

    #[test]
    fn sec_pr_comment_escapes_markdown_links() {
        let output = make_evil_output();
        let formatter = PrCommentFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("[inject](https://evil.com)"),
            "PR comment should escape link injection");
    }

    #[test]
    fn sec_pr_comment_escapes_newlines_in_cells() {
        let output = make_evil_output();
        let formatter = PrCommentFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("\n\n# Headline"),
            "PR comment should escape newlines in table cells");
    }

    #[test]
    fn sec_pr_comment_escapes_circular_dep_files() {
        let output = make_evil_output();
        let formatter = PrCommentFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("[evil](https://x)"),
            "PR comment should escape file names in circular deps");
    }

    // ── V5-4: escape_md_cell must escape backticks and angle brackets ──
    #[test]
    fn sec_v5_4_pr_comment_escapes_backticks_and_angle_brackets() {
        let cell = escape_md_cell("file`name<evil>.ts");
        assert_eq!(cell, "file\\`name&lt;evil&gt;.ts",
            "backticks and angle brackets must be escaped, got: {}", cell);
    }
}
