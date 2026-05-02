//! Duplication analysis: clone groups, families, stats, mirrored directories.

mod families;
mod groups;
mod mirrored;
mod ncd;
mod stats;

pub use families::build_clone_families;
pub use groups::build_clone_groups;
pub use mirrored::detect_mirrored_directories;
pub use ncd::{NcdCandidate, detect_ncd_duplicates, find_candidate_pairs, ncd};
pub mod patterns;
pub use stats::compute_duplication_stats;

use crate::types::{DuplicateCodeIssue, DuplicationSection, RepetitivePattern};

/// Build the full duplication section from raw duplicate-code issues.
pub fn build_duplication_section(
    issues: &[DuplicateCodeIssue],
    total_source_lines: usize,
    patterns: Vec<RepetitivePattern>,
) -> DuplicationSection {
    let clone_groups = build_clone_groups(issues);
    let clone_families = build_clone_families(&clone_groups);
    let mirrored_directories = detect_mirrored_directories(&clone_groups);
    let stats = compute_duplication_stats(&clone_groups, &clone_families, total_source_lines);
    DuplicationSection { stats, clone_groups, clone_families, mirrored_directories, repetitive_patterns: patterns }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CodeBlockLocation;

    fn make_issue(
        file_a: &str,
        start_a: usize,
        end_a: usize,
        file_b: &str,
        start_b: usize,
        end_b: usize,
    ) -> DuplicateCodeIssue {
        DuplicateCodeIssue {
            confidence: 0.9,
            location_a: CodeBlockLocation {
                file: file_a.to_string(),
                name: "fn".to_string(),
                start_line: start_a,
                end_line: end_a,
                snippet: "code".to_string(),
            },
            location_b: CodeBlockLocation {
                file: file_b.to_string(),
                name: "fn".to_string(),
                start_line: start_b,
                end_line: end_b,
                snippet: "code".to_string(),
            },
        }
    }

    #[test]
    fn empty_issues_produces_empty_section() {
        let section = build_duplication_section(&[], 1000, vec![]);
        assert_eq!(section.stats.total_lines, 1000);
        assert_eq!(section.stats.duplicated_lines, 0);
        assert!(section.clone_groups.is_empty());
        assert!(section.clone_families.is_empty());
        assert!(section.mirrored_directories.is_empty());
    }

    #[test]
    fn single_issue_produces_one_group() {
        let issues = vec![make_issue("a.ts", 1, 10, "b.ts", 5, 14)];
        let section = build_duplication_section(&issues, 500, vec![]);
        assert_eq!(section.clone_groups.len(), 1);
        assert_eq!(section.clone_groups[0].instances.len(), 2);
        assert_eq!(section.clone_groups[0].line_count, 10);
    }
}
