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
        md.push('\n');

        // Health Dashboard
        md.push_str("## Health Dashboard\n\n");
        let score = summary.health_score;
        let emoji = if score >= 80.0 {
            "🟢"
        } else if score >= 50.0 {
            "🟡"
        } else {
            "🔴"
        };
        md.push_str(&format!("**Overall Health Score: {} {:.1}/100**\n\n", emoji, score));
        md.push_str(&health_bar(score));
        md.push('\n');

        // Dead Code
        if !output.issues.dead_code.is_empty() {
            md.push_str("## Dead Code\n\n");
            md.push_str("| File | Lines | Confidence | Reason |\n|---|---|---|---|\n");
            let mut sorted = output.issues.dead_code.clone();
            sorted.sort_by(|a, b| b.lines_of_code.cmp(&a.lines_of_code));
            for dc in sorted.iter().take(50) {
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

        // Unused Exports
        if !output.issues.unused_exports.is_empty() {
            md.push_str("## Unused Exports (Top 20)\n\n");
            md.push_str("| Export | File |\n|---|---|\n");
            for ue in output.issues.unused_exports.iter().take(20) {
                md.push_str(&format!("| `{}` | `{}` |\n", escape_md_cell(&ue.name), escape_md_cell(&ue.path)));
            }
            md.push('\n');
        }

        // Unused Types
        if !output.issues.unused_types.is_empty() {
            md.push_str("## Unused Types (Top 20)\n\n");
            md.push_str("| Type | Kind | File |\n|---|---|---|\n");
            for ut in output.issues.unused_types.iter().take(20) {
                md.push_str(&format!("| `{}` | {} | `{}` |\n", escape_md_cell(&ut.name), ut.kind, escape_md_cell(&ut.path)));
            }
            md.push('\n');
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
                let files: Vec<String> =
                    g.instances.iter().map(|inst| format!("{}:L{}", escape_md_cell(&inst.file), inst.start_line)).collect();
                md.push_str(&format!("| {} | {} | {} |\n", i + 1, files.join(", "), g.line_count));
            }
            md.push('\n');

            if !output.duplication.clone_families.is_empty() {
                md.push_str("### Clone Families\n\n");
                for fam in &output.duplication.clone_families {
                let escaped_fam_files: Vec<String> = fam.files.iter().map(|f| escape_md_cell(f)).collect();
                    md.push_str(&format!(
                        "- **{} groups, {} lines**: {}\n",
                        fam.group_count,
                        fam.total_duplicated_lines,
                        escaped_fam_files.join(", ")
                    ));
                }
                md.push('\n');
            }
        }

        // Circular Dependencies
        if !output.issues.circular_dependencies.is_empty() {
            md.push_str("## Circular Dependencies\n\n");
            for cd in &output.issues.circular_dependencies {
                let escaped_files: Vec<String> = cd.files.iter().map(|f| escape_md_cell(f)).collect();
                md.push_str(&format!("- {} → {}\n", escaped_files.join(" → "), escaped_files[0]));
            }
            md.push('\n');
        }

        // Framework Info
        md.push_str("## Framework Info\n\n");
        md.push_str(&format!("- **Entry points:** {}\n", output.structure.entry_points.len()));
        md.push_str(&format!("- **Config files:** {}\n", output.structure.config_files.iter().map(|f| escape_md_cell(f)).collect::<Vec<_>>().join(", ")));

        Ok(md)
    }
}

fn health_bar(score: f64) -> String {
    let filled = (score / 5.0).round() as usize;
    let empty = 20 - filled;
    format!("`[{}{}]`", "#".repeat(filled), "-".repeat(empty))
}

/// Escape special characters for safe embedding in Markdown table cells.
/// Prevents injection of links, table breaks, backtick breaks, and structural elements.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_output_with_evil_path() -> AnalysisOutput {
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
                    path: "src/evil | [link](https://evil.com) | ".to_string(),
                    lines_of_code: 42,
                    confidence: 0.9,
                    reason: "unused | [inject](https://evil.com)".to_string(),
                }],
                unused_exports: vec![UnusedExportIssue {
                    name: "EvilExport\n\n# Headline".to_string(),
                    path: "src/x | [evil](https://evil.com)".to_string(),
                }],
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
                    total_lines: 0,
                    duplicated_lines: 0,
                    duplication_percentage: 0.0,
                    clone_groups: 0,
                    clone_instances: 0,
                    clone_families: 0,
                },
                clone_groups: vec![],
                clone_families: vec![],
                mirrored_directories: vec![],
                repetitive_patterns: vec![],
            },
        }
    }

    #[test]
    fn sec_markdown_escapes_pipe_in_tables() {
        // Pipe characters in file paths must be escaped to prevent table injection
        let output = make_output_with_evil_path();
        let formatter = MarkdownFormatter;
        let md = formatter.format(&output).unwrap();
        // Raw | in table cell values would break markdown table formatting
        assert!(!md.contains("evil | [link]"),
            "markdown should escape pipe chars in table cells: {}", md);
    }

    #[test]
    fn sec_markdown_escapes_links() {
        let output = make_output_with_evil_path();
        let formatter = MarkdownFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("[link](https://evil.com)"),
            "markdown should escape link injection in cells");
    }

    #[test]
    fn sec_markdown_escapes_newlines_in_cells() {
        let output = make_output_with_evil_path();
        let formatter = MarkdownFormatter;
        let md = formatter.format(&output).unwrap();
        // Newlines in table cells break the table structure
        assert!(!md.contains("EvilExport\n\n# Headline"),
            "markdown should escape newlines in table cells");
    }

    // ── V4-5: Backtick injection breaks inline code spans ──
    #[test]
    fn sec_markdown_escapes_backticks_in_cells() {
        let mut output = make_output_with_evil_path();
        output.issues.dead_code = vec![DeadCodeIssue {
            path: "src/evil`code`.ts".to_string(),
            lines_of_code: 10,
            confidence: 0.9,
            reason: "`rm -rf /`".to_string(),
        }];
        let md = MarkdownFormatter.format(&output).unwrap();
        // Raw backticks should be escaped so they don't break table code spans
        assert!(md.contains("\\`code\\`"), "backticks should be escaped in table cells, got:\n{}", md);
    }

    // ── V4-6: Circular dependency file names not escaped ──
    #[test]
    fn sec_markdown_escapes_circular_dep_files() {
        let mut output = make_output_with_evil_path();
        output.issues.dead_code = vec![];
        output.issues.unused_exports = vec![];
        output.issues.circular_dependencies = vec![CircularDepIssue {
            files: vec!["src/[evil](https://evil.com).ts".to_string(), "src/b.ts".to_string()],
        }];
        let md = MarkdownFormatter.format(&output).unwrap();
        assert!(!md.contains("[evil](https://evil.com)"),
            "circular dep file names should be escaped, got:\n{}", md);
    }

    // ── V4-7: Duplication instance file names not escaped ──
    #[test]
    fn sec_markdown_escapes_duplication_files() {
        let mut output = make_output_with_evil_path();
        output.issues.dead_code = vec![];
        output.issues.unused_exports = vec![];
        output.duplication.clone_groups = vec![CloneGroup {
            instances: vec![
                CloneInstance { file: "src/[evil](https://evil.com).ts".to_string(), start_line: 1, end_line: 10, snippet: "...".to_string() },
                CloneInstance { file: "src/b.ts".to_string(), start_line: 1, end_line: 10, snippet: "...".to_string() },
            ],
            token_count: 60,
            line_count: 10,
        }];
        let md = MarkdownFormatter.format(&output).unwrap();
        assert!(!md.contains("[evil](https://evil.com)"),
            "duplication file names should be escaped, got:\n{}", md);
    }

    // ── V4-10: HTML chars not escaped in markdown cells ──
    #[test]
    fn sec_markdown_escapes_html_chars_in_cells() {
        let mut output = make_output_with_evil_path();
        output.issues.dead_code = vec![DeadCodeIssue {
            path: "src/<script>alert(1)</script>.ts".to_string(),
            lines_of_code: 10,
            confidence: 0.9,
            reason: "<b>bold</b>".to_string(),
        }];
        let md = MarkdownFormatter.format(&output).unwrap();
        assert!(!md.contains("<script>"),
            "HTML angle brackets should be escaped in markdown, got:\n{}", md);
        assert!(md.contains("&lt;script&gt;"),
            "should use HTML entities for angle brackets, got:\n{}", md);
    }

    // ── V5-9: config_files are escaped in markdown output ──
    #[test]
    fn sec_markdown_config_files_escaped() {
        let output = AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: std::path::PathBuf::from("/tmp/test"),
                entry_points: vec![],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec!["src/[evil](link).toml".to_string()],
            },
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            issues: Issues {
                dead_code: vec![], unused_exports: vec![], duplicate_exports: vec![],
                duplicate_code: vec![], gotchas: vec![], unused_types: vec![],
                circular_dependencies: vec![], unused_dependencies: vec![],
                unresolved_imports: vec![], unlisted_dependencies: vec![], plugin_issues: vec![],
            },
            duplication: DuplicationSection {
                stats: DuplicationStats {
                    total_lines: 0, duplicated_lines: 0,
                    duplication_percentage: 0.0, clone_groups: 0,
                    clone_instances: 0, clone_families: 0,
                },
                clone_groups: vec![], clone_families: vec![], mirrored_directories: vec![],
                repetitive_patterns: vec![],
            },
        };
        let formatter = MarkdownFormatter;
        let md = formatter.format(&output).unwrap();
        assert!(!md.contains("[evil](link)"),
            "config_files should be escaped, got:\n{}", md);
    }
}
