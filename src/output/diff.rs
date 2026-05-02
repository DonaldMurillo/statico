//! Diff and trend report support.
//!
//! Compares two analysis outputs to identify new, fixed, and persisting issues.

use crate::types::AnalysisOutput;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Result of comparing two analysis outputs.
#[derive(Debug, Serialize)]
pub struct DiffResult {
    pub new_issues: Vec<DiffEntry>,
    pub fixed_issues: Vec<DiffEntry>,
    pub persisting: Vec<DiffEntry>,
}

/// A single diff entry identifying an issue by category and key.
#[derive(Debug, Clone, Serialize)]
pub struct DiffEntry {
    pub category: String,
    pub key: String,
    pub detail: String,
}

/// Compute the diff between two analysis outputs.
pub fn compute_diff(before: &AnalysisOutput, after: &AnalysisOutput) -> DiffResult {
    let before_keys = collect_issue_keys(before);
    let after_keys = collect_issue_keys(after);

    let before_set: HashSet<(String, String)> =
        before_keys.iter().map(|e| (e.category.clone(), e.key.clone())).collect();
    let after_set: HashSet<(String, String)> = after_keys.iter().map(|e| (e.category.clone(), e.key.clone())).collect();

    let new_issues: Vec<DiffEntry> =
        after_keys.iter().filter(|e| !before_set.contains(&(e.category.clone(), e.key.clone()))).cloned().collect();

    let fixed_issues: Vec<DiffEntry> =
        before_keys.iter().filter(|e| !after_set.contains(&(e.category.clone(), e.key.clone()))).cloned().collect();

    let persisting: Vec<DiffEntry> =
        after_keys.iter().filter(|e| before_set.contains(&(e.category.clone(), e.key.clone()))).cloned().collect();

    DiffResult { new_issues, fixed_issues, persisting }
}

/// Collect all issue keys from an analysis output.
fn collect_issue_keys(output: &AnalysisOutput) -> Vec<DiffEntry> {
    let mut entries = Vec::new();

    for dc in &output.issues.dead_code {
        entries.push(DiffEntry {
            category: "dead_code".into(),
            key: dc.path.clone(),
            detail: format!("{} ({} loc, {:.0}%)", dc.path, dc.lines_of_code, dc.confidence * 100.0),
        });
    }

    for ue in &output.issues.unused_exports {
        entries.push(DiffEntry {
            category: "unused_export".into(),
            key: format!("{}::{}", ue.path, ue.name),
            detail: format!("{} in {}", ue.name, ue.path),
        });
    }

    for ut in &output.issues.unused_types {
        entries.push(DiffEntry {
            category: "unused_type".into(),
            key: format!("{}::{}", ut.path, ut.name),
            detail: format!("{} {} in {}", ut.kind, ut.name, ut.path),
        });
    }

    for dc in &output.issues.duplicate_code {
        let key = format!(
            "{}:L{}-{}::{}:L{}-{}",
            dc.location_a.file,
            dc.location_a.start_line,
            dc.location_a.end_line,
            dc.location_b.file,
            dc.location_b.start_line,
            dc.location_b.end_line,
        );
        entries.push(DiffEntry {
            category: "duplicate_code".into(),
            key,
            detail: format!("{} vs {}", dc.location_a.file, dc.location_b.file),
        });
    }

    for g in &output.issues.gotchas {
        entries.push(DiffEntry {
            category: "gotcha".into(),
            key: format!("{}:{}:{}", g.file, g.line, g.rule),
            detail: format!("{}:{} [{}] {}", g.file, g.line, g.rule, g.message),
        });
    }

    for cd in &output.issues.circular_dependencies {
        entries.push(DiffEntry {
            category: "circular_dependency".into(),
            key: cd.files.join("→"),
            detail: cd.files.join(" → "),
        });
    }

    for de in &output.issues.duplicate_exports {
        let mut locs = de.locations.clone();
        locs.sort();
        entries.push(DiffEntry {
            category: "duplicate_export".into(),
            key: format!("{}::{}", de.name, locs.join(",")),
            detail: format!("{} in {}", de.name, locs.join(", ")),
        });
    }

    for ui in &output.issues.unresolved_imports {
        entries.push(DiffEntry {
            category: "unresolved_import".into(),
            key: format!("{}::{}", ui.source_file, ui.import_spec),
            detail: format!("{} in {}", ui.import_spec, ui.source_file),
        });
    }

    for ud in &output.issues.unused_dependencies {
        entries.push(DiffEntry {
            category: "unused_dependency".into(),
            key: ud.package_name.clone(),
            detail: ud.package_name.clone(),
        });
    }

    for ud in &output.issues.unlisted_dependencies {
        entries.push(DiffEntry {
            category: "unlisted_dependency".into(),
            key: format!("{}::{}", ud.imported_by, ud.package_name),
            detail: format!("{} imported by {}", ud.package_name, ud.imported_by),
        });
    }

    entries
}

/// Format diff result as JSON.
pub fn format_diff_json(diff: &DiffResult) -> Result<String, String> {
    serde_json::to_string_pretty(diff).map_err(|e| format!("failed to serialize diff: {}", e))
}

/// Escape special characters for safe embedding in Markdown table cells.
fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|")
     .replace('[', "\\[")
     .replace(']', "\\]")
     .replace('`', "\\`")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace(['\n', '\r'], " ")
}

/// Format diff result as Markdown.
pub fn format_diff_markdown(diff: &DiffResult) -> Result<String, String> {
    let mut md = String::new();
    md.push_str("# statico Diff Report\n\n");

    md.push_str(&format!("**New issues:** {}\n", diff.new_issues.len()));
    md.push_str(&format!("**Fixed issues:** {}\n", diff.fixed_issues.len()));
    md.push_str(&format!("**Persisting issues:** {}\n\n", diff.persisting.len()));

    if !diff.new_issues.is_empty() {
        md.push_str("## 🆕 New Issues\n\n");
        md.push_str("| Category | Detail |\n|---|---|\n");
        for e in &diff.new_issues {
            md.push_str(&format!("| {} | {} |\n", escape_md_cell(&e.category), escape_md_cell(&e.detail)));
        }
        md.push('\n');
    }

    if !diff.fixed_issues.is_empty() {
        md.push_str("## ✅ Fixed Issues\n\n");
        md.push_str("| Category | Detail |\n|---|---|\n");
        for e in &diff.fixed_issues {
            md.push_str(&format!("| {} | {} |\n", escape_md_cell(&e.category), escape_md_cell(&e.detail)));
        }
        md.push('\n');
    }

    if !diff.persisting.is_empty() {
        md.push_str(&format!("## ⏳ Persisting Issues ({})\n\n", diff.persisting.len()));
        if diff.persisting.len() <= 30 {
            md.push_str("| Category | Detail |\n|---|---|\n");
            for e in &diff.persisting {
                md.push_str(&format!("| {} | {} |\n", escape_md_cell(&e.category), escape_md_cell(&e.detail)));
            }
        }
        md.push('\n');
    }

    // Summary by category
    let mut new_by_cat: HashMap<String, usize> = HashMap::new();
    let mut fixed_by_cat: HashMap<String, usize> = HashMap::new();
    for e in &diff.new_issues {
        *new_by_cat.entry(e.category.clone()).or_insert(0) += 1;
    }
    for e in &diff.fixed_issues {
        *fixed_by_cat.entry(e.category.clone()).or_insert(0) += 1;
    }

    md.push_str("## Summary by Category\n\n");
    md.push_str("| Category | New | Fixed | Net |\n|---|---|---|---|\n");
    let all_cats: HashSet<&str> = new_by_cat.keys().chain(fixed_by_cat.keys()).map(|s| s.as_str()).collect();
    let mut cats: Vec<&str> = all_cats.into_iter().collect();
    cats.sort();
    for cat in cats {
        let n = new_by_cat.get(cat).copied().unwrap_or(0);
        let f = fixed_by_cat.get(cat).copied().unwrap_or(0);
        let net: i64 = n as i64 - f as i64;
        let sign = if net > 0 { "+" } else { "" };
        md.push_str(&format!("| {} | {} | {} | {}{} |\n", cat, n, f, sign, net));
    }

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_output_with_dead(dead: Vec<DeadCodeIssue>) -> AnalysisOutput {
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
                dead_code: dead,
                unused_exports: vec![],
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
    fn sec_diff_escapes_pipe_in_detail() {
        let old = make_output_with_dead(vec![DeadCodeIssue {
            path: "src/a.ts".into(),
            lines_of_code: 10,
            confidence: 0.9,
            reason: "old".into(),
        }]);
        let new = make_output_with_dead(vec![DeadCodeIssue {
            path: "src/a|ts [link](https://evil)".into(),
            lines_of_code: 10,
            confidence: 0.9,
            reason: "injected".into(),
        }]);
        let diff = compute_diff(&old, &new);
        let md = format_diff_markdown(&diff).unwrap();
        // Raw pipe should be escaped
        assert!(!md.contains("| src/a|ts"),
            "pipe in detail should be escaped, got:\n{}", md);
        assert!(md.contains("\\|") || md.contains("src/a"),
            "escaped pipe should be present");
        // Raw markdown link should not appear
        assert!(!md.contains("[link](https://evil)"),
            "markdown link in detail should be escaped, got:\n{}", md);
    }

    #[test]
    fn sec_diff_escapes_newlines_in_detail() {
        let old = make_output_with_dead(vec![]);
        let new = make_output_with_dead(vec![DeadCodeIssue {
            path: "src/b.ts".into(),
            lines_of_code: 5,
            confidence: 0.8,
            reason: "line1\nline2\nline3".into(),
        }]);
        let diff = compute_diff(&old, &new);
        let md = format_diff_markdown(&diff).unwrap();
        // Newlines in detail should be replaced with spaces
        assert!(!md.contains("line1\nline2"),
            "newlines in table cells should be escaped, got:\n{}", md);
    }

    // ── V5-7: escape_md_cell must escape backticks and angle brackets ──
    #[test]
    fn sec_diff_escapes_backticks_and_angle_brackets() {
        let cell = escape_md_cell("file`code`<evil>.ts");
        assert_eq!(cell, "file\\`code\\`&lt;evil&gt;.ts",
            "backticks and angle brackets must be escaped, got: {}", cell);
    }
}
