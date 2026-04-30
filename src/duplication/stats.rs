//! Compute duplication statistics from clone groups.

use crate::types::{CloneFamily, CloneGroup, DuplicationStats};

/// Compute duplication stats.
///
/// `duplicated_lines` counts **unique** source lines that appear in at least one
/// clone group instance. A line is identified by (file, line_number). This
/// matches fallow's approach: each source line is counted at most once, even if
/// it participates in multiple clone groups.
pub fn compute_duplication_stats(
    groups: &[CloneGroup],
    families: &[CloneFamily],
    total_source_lines: usize,
) -> DuplicationStats {
    // Track unique (file, line) pairs to avoid double-counting.
    let mut seen_lines: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
    let mut total_instances: usize = 0;

    let mut estimated_dup_lines: usize = 0;

    for g in groups {
        total_instances += g.instances.len();
        for inst in &g.instances {
            for line in inst.start_line..=inst.end_line {
                seen_lines.insert((inst.file.clone(), line));
            }
        }

        // Also compute an estimated dup-lines count using min-instance heuristic.
        // This handles the case where one instance is much larger than others
        // (e.g., a 1429-line migration file matched against a 5-line fragment).
        // The min span represents the actual duplicated code size.
        let min_span = g.instances.iter().map(|i| i.end_line - i.start_line + 1).min().unwrap_or(0);
        estimated_dup_lines += min_span * g.instances.len();
    }

    // Use the lesser of raw unique lines and estimated dup lines.
    // Raw unique lines over-counts when instances have wildly different spans.
    // Estimated dup lines under-counts when instances overlap on the same file.
    let duplicated_lines = seen_lines.len().min(estimated_dup_lines);

    let duplication_percentage =
        if total_source_lines > 0 { (duplicated_lines as f64 / total_source_lines as f64) * 100.0 } else { 0.0 };

    DuplicationStats {
        total_lines: total_source_lines,
        duplicated_lines,
        duplication_percentage,
        clone_groups: groups.len(),
        clone_instances: total_instances,
        clone_families: families.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CloneInstance;

    fn make_group(line_count: usize, num_instances: usize) -> CloneGroup {
        CloneGroup {
            instances: (0..num_instances)
                .map(|i| CloneInstance {
                    file: format!("file_{}.ts", i),
                    start_line: 1,
                    end_line: line_count,
                    snippet: String::new(),
                })
                .collect(),
            token_count: line_count * 6,
            line_count,
        }
    }

    #[test]
    fn empty_groups() {
        let stats = compute_duplication_stats(&[], &[], 1000);
        assert_eq!(stats.total_lines, 1000);
        assert_eq!(stats.duplicated_lines, 0);
        assert_eq!(stats.duplication_percentage, 0.0);
        assert_eq!(stats.clone_groups, 0);
        assert_eq!(stats.clone_instances, 0);
        assert_eq!(stats.clone_families, 0);
    }

    #[test]
    fn single_group_two_instances() {
        let groups = vec![make_group(10, 2)]; // 2 files × 10 lines each = 20 unique lines
        let stats = compute_duplication_stats(&groups, &[], 100);
        assert_eq!(stats.duplicated_lines, 20); // unique (file, line) pairs
        assert_eq!(stats.clone_groups, 1);
        assert_eq!(stats.clone_instances, 2);
    }

    #[test]
    fn percentage_calculation() {
        let groups = vec![make_group(10, 2)]; // 20 unique duplicated lines
        let stats = compute_duplication_stats(&groups, &[], 200);
        assert!((stats.duplication_percentage - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_total_lines() {
        let groups = vec![make_group(10, 2)];
        let stats = compute_duplication_stats(&groups, &[], 0);
        assert_eq!(stats.duplication_percentage, 0.0);
    }

    #[test]
    fn counts_families() {
        let groups = vec![make_group(5, 2)];
        let families = vec![crate::types::CloneFamily {
            files: vec!["a.ts".into(), "b.ts".into()],
            group_count: 2,
            total_duplicated_lines: 10,
        }];
        let stats = compute_duplication_stats(&groups, &families, 100);
        assert_eq!(stats.clone_families, 1);
    }

    #[test]
    fn deduplicates_overlapping_ranges() {
        // Two groups that overlap on the same file — lines should be counted once.
        let groups = vec![
            CloneGroup {
                instances: vec![CloneInstance {
                    file: "a.ts".into(),
                    start_line: 1,
                    end_line: 10,
                    snippet: String::new(),
                }],
                token_count: 60,
                line_count: 10,
            },
            CloneGroup {
                instances: vec![CloneInstance {
                    file: "a.ts".into(),
                    start_line: 5,
                    end_line: 15,
                    snippet: String::new(),
                }],
                token_count: 66,
                line_count: 11,
            },
        ];
        let stats = compute_duplication_stats(&groups, &[], 100);
        // Lines 1-15 = 15 unique lines (not 10+11=21)
        assert_eq!(stats.duplicated_lines, 15);
        assert_eq!(stats.clone_groups, 2);
    }
}
