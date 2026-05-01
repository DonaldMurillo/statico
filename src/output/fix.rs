//! Fix/dry-run output formatter for `statico --fix --dry-run`.
//!
//! Generates actionable comment-based hints (shell comments) describing what
//! would be fixed. Each line is prefixed with `#` so the output is clearly
//! a suggestion, not executable code.

use std::collections::BTreeMap;

use super::OutputFormatter;
use crate::types::AnalysisOutput;

/// Minimum confidence required for an issue to be included in fix suggestions.
const DEFAULT_MIN_CONFIDENCE: f64 = 0.8;

/// Formatter that produces a comment-based hint list for safe automatic fixes.
///
/// Intended for `--fix --dry-run` mode. The output is plain text where every
/// line starts with `#`, making it safe to redirect into a shell script or
/// review as a patch preview.
pub struct FixFormatter;

impl OutputFormatter for FixFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let mut out = String::new();

        // Collect high-confidence dead code issues, sorted by confidence descending.
        let mut dead_files: Vec<_> =
            output.issues.dead_code.iter().filter(|dc| dc.confidence >= DEFAULT_MIN_CONFIDENCE).collect();
        dead_files.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.lines_of_code.cmp(&a.lines_of_code))
        });

        let total_dead_loc: usize = dead_files.iter().map(|dc| dc.lines_of_code).sum();

        // Unused exports — no confidence field, so we include all of them.
        let unused_exports = &output.issues.unused_exports;
        let unused_export_count = unused_exports.len();

        // --- Summary header ---
        out.push_str("# statico --fix --dry-run\n");
        out.push_str(&format!(
            "# Would remove: {} dead files ({} LOC), {} unused exports\n",
            dead_files.len(),
            total_dead_loc,
            unused_export_count,
        ));
        out.push_str(&format!(
            "# Confidence filter: >= {:.1} (use --min-confidence to adjust)\n",
            DEFAULT_MIN_CONFIDENCE,
        ));
        out.push_str(
            "# \u{26A0}\u{FE0F}  Review each change before applying. This is a suggestion, not a guarantee.\n",
        );

        // --- Dead file removal hints ---
        if !dead_files.is_empty() {
            out.push('\n');
            out.push_str("# === DEAD FILES (safe to delete) ===\n");
            out.push('\n');

            for dc in &dead_files {
                let safe_path = dc.path.replace('\n', " ").replace('\r', " ");
                let safe_reason = dc.reason.replace('\n', " ").replace('\r', " ");
                out.push_str(&format!(
                    "# SAFE TO DELETE: {} ({} LOC, confidence: {:.0}%)\n",
                    safe_path,
                    dc.lines_of_code,
                    dc.confidence * 100.0,
                ));
                out.push_str(&format!("# Reason: {}\n", safe_reason));
                out.push_str(&format!("# Review: git show HEAD -- {}\n", safe_path));
                out.push('\n');
            }
        }

        // --- Unused export removal hints, grouped by file ---
        if !unused_exports.is_empty() {
            out.push_str("# === UNUSED EXPORTS (safe to remove) ===\n");
            out.push('\n');

            let by_file = group_unused_exports_by_file(unused_exports);
            for (path, exports) in &by_file {
                let safe_path = path.replace('\n', " ").replace('\r', " ");
                out.push_str(&format!("# File: {}\n", safe_path));
                for name in exports {
                    let safe_name = name.replace('\n', " ").replace('\r', " ");
                    out.push_str(&format!("# REMOVE EXPORT: {} from {}\n", safe_name, safe_path));
                    out.push_str("# This export is defined but never imported by any file\n");
                }
                out.push('\n');
            }
        }

        // If nothing to fix, say so.
        if dead_files.is_empty() && unused_exports.is_empty() {
            out.push('\n');
            out.push_str("# No actionable fixes found at the current confidence threshold.\n");
            out.push_str("# Try lowering --min-confidence to discover more suggestions.\n");
        }

        Ok(out)
    }
}

/// Group unused export issues by their file path, sorting exports alphabetically
/// within each file for deterministic output.
fn group_unused_exports_by_file(exports: &[crate::types::UnusedExportIssue]) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for exp in exports {
        map.entry(exp.path.clone()).or_default().push(exp.name.clone());
    }
    // Sort export names within each file for deterministic ordering.
    for names in map.values_mut() {
        names.sort();
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_output(dead_code: Vec<DeadCodeIssue>, unused_exports: Vec<UnusedExportIssue>) -> AnalysisOutput {
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
                dead_code,
                unused_exports,
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
            },
        }
    }

    #[test]
    fn test_nothing_to_fix() {
        let output = make_output(vec![], vec![]);
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        assert!(result.contains("No actionable fixes found"));
    }

    #[test]
    fn test_dead_file_hint() {
        let output = make_output(
            vec![DeadCodeIssue {
                path: "src/dead.ts".into(),
                lines_of_code: 42,
                confidence: 0.95,
                reason: "Not reachable from any entry point".into(),
            }],
            vec![],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        assert!(result.contains("SAFE TO DELETE: src/dead.ts (42 LOC, confidence: 95%)"));
        assert!(result.contains("# Reason: Not reachable from any entry point"));
        assert!(result.contains("# Review: git show HEAD -- src/dead.ts"));
    }

    #[test]
    fn test_filters_low_confidence() {
        let output = make_output(
            vec![DeadCodeIssue {
                path: "src/maybe.ts".into(),
                lines_of_code: 10,
                confidence: 0.5,
                reason: "Low confidence".into(),
            }],
            vec![],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        assert!(result.contains("Would remove: 0 dead files"));
        assert!(!result.contains("src/maybe.ts"));
    }

    #[test]
    fn test_unused_exports_grouped_by_file() {
        let output = make_output(
            vec![],
            vec![
                UnusedExportIssue { name: "foo".into(), path: "src/a.ts".into() },
                UnusedExportIssue { name: "bar".into(), path: "src/b.ts".into() },
                UnusedExportIssue { name: "baz".into(), path: "src/a.ts".into() },
            ],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        assert!(result.contains("REMOVE EXPORT: baz from src/a.ts"));
        assert!(result.contains("REMOVE EXPORT: foo from src/a.ts"));
        assert!(result.contains("REMOVE EXPORT: bar from src/b.ts"));
        // Verify grouping: both baz and foo appear under src/a.ts
        let a_section = result.split("File: src/a.ts").nth(1).unwrap();
        assert!(a_section.contains("REMOVE EXPORT: baz"));
        assert!(a_section.contains("REMOVE EXPORT: foo"));
    }

    #[test]
    fn test_dead_files_sorted_by_confidence() {
        let output = make_output(
            vec![
                DeadCodeIssue { path: "src/low.ts".into(), lines_of_code: 100, confidence: 0.85, reason: "Low".into() },
                DeadCodeIssue {
                    path: "src/high.ts".into(),
                    lines_of_code: 50,
                    confidence: 0.99,
                    reason: "High".into(),
                },
            ],
            vec![],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        let high_pos = result.find("src/high.ts").unwrap();
        let low_pos = result.find("src/low.ts").unwrap();
        assert!(high_pos < low_pos, "Higher confidence file should appear first");
    }

    // ── V4-2: Newline injection via path field ──
    #[test]
    fn sec_v4_2_fix_formatter_strips_newlines_in_path() {
        let output = make_output(
            vec![DeadCodeIssue {
                path: "src/good.ts\nevil-command\n".to_string(),
                lines_of_code: 10,
                confidence: 0.95,
                reason: "unused".to_string(),
            }],
            vec![],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        // A newline in the path should not inject a fake comment line
        assert!(result.contains("src/good.ts evil-command "),
            "newlines in path should be replaced with spaces, got:\n{}", result);
        assert!(!result.contains("\nevil-command\n"),
            "raw newlines should not appear in path output, got:\n{}", result);
    }

    // ── V4-3: Newline injection via reason field ──
    #[test]
    fn sec_v4_3_fix_formatter_strips_newlines_in_reason() {
        let output = make_output(
            vec![DeadCodeIssue {
                path: "src/dead.ts".to_string(),
                lines_of_code: 10,
                confidence: 0.95,
                reason: "not reachable\n# SAFE TO DELETE: /etc/passwd".to_string(),
            }],
            vec![],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        // The injected text should NOT appear as its own "# SAFE TO DELETE:" line
        let safe_delete_lines: Vec<_> = result.lines()
            .filter(|l| l.starts_with("# SAFE TO DELETE:"))
            .collect();
        assert_eq!(safe_delete_lines.len(), 1, "should only have one real SAFE TO DELETE line, got:\n{:?}\n{}",
            safe_delete_lines, result);
        assert!(safe_delete_lines[0].contains("src/dead.ts"),
            "the real SAFE TO DELETE should reference the actual file, got:\n{}", result);
        // Reason content should be on one line with newlines replaced
        let reason_line = result.lines().find(|l| l.starts_with("# Reason:")).unwrap();
        assert!(reason_line.contains("not reachable # SAFE TO DELETE: /etc/passwd"),
            "reason should have newlines replaced with spaces, got:\n{}", reason_line);
    }

    // ── V6-2: Newline injection via unused export path (group header) ──
    #[test]
    fn sec_v6_2_fix_formatter_strips_newlines_in_export_path() {
        let output = make_output(
            vec![],
            vec![
                UnusedExportIssue { name: "foo".into(), path: "src/a.ts\n# FAKE DIRECTIVE\n".into() },
            ],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        // Should NOT have a bare "# FAKE DIRECTIVE" line injected by the path
        let fake_lines: Vec<_> = result.lines()
            .filter(|l| l.trim() == "# FAKE DIRECTIVE")
            .collect();
        assert!(fake_lines.is_empty(),
            "newline in export path should not inject fake directives, got:\n{:?}\n{}",
            fake_lines, result);
        // Path should have newlines replaced with spaces
        assert!(result.contains("src/a.ts # FAKE DIRECTIVE"),
            "path should have newlines replaced with spaces, got:\n{}", result);
    }

    // ── V6-3: Newline injection via unused export name ──
    #[test]
    fn sec_v6_3_fix_formatter_strips_newlines_in_export_name() {
        let output = make_output(
            vec![],
            vec![
                UnusedExportIssue { name: "myExport\n# FAKE REMOVE".into(), path: "src/a.ts".into() },
            ],
        );
        let fmt = FixFormatter;
        let result = fmt.format(&output).unwrap();
        // Should NOT have a bare "# FAKE REMOVE" line injected by the name
        let fake_lines: Vec<_> = result.lines()
            .filter(|l| l.trim() == "# FAKE REMOVE")
            .collect();
        assert!(fake_lines.is_empty(),
            "newline in export name should not inject fake directives, got:\n{:?}\n{}",
            fake_lines, result);
    }
}
