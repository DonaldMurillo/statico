//! Transform `DuplicateCodeIssue` list into structured `CloneGroup`s.

use std::collections::HashMap;

use crate::types::{CloneGroup, CloneInstance, CodeBlockLocation, DuplicateCodeIssue};

/// Token heuristic: approximate tokens per line of code.
const TOKENS_PER_LINE: usize = 6;

/// Build clone groups from raw duplicate-code issues.
///
/// Each `DuplicateCodeIssue` maps to one `CloneGroup` with 2 instances.
/// Issues that share the same line range and snippet in at least one file
/// are merged into a single group with all unique instances.
pub fn build_clone_groups(issues: &[DuplicateCodeIssue]) -> Vec<CloneGroup> {
    if issues.is_empty() {
        return vec![];
    }

    // Strategy: use a union-find–like approach.
    // Two issues are in the same group if they share a location with the same
    // (file, start_line, end_line) tuple. We build connected components.
    //
    // Simplified approach: assign each issue an ID, then union issues that
    // share a (file, start, end) location. Finally, collect instances per group.

    // Map: (file, start, end) -> issue indices that contain this location.
    let mut location_to_issues: HashMap<(String, usize, usize), Vec<usize>> = HashMap::new();

    for (idx, issue) in issues.iter().enumerate() {
        let a = &issue.location_a;
        let b = &issue.location_b;

        let key_a = (a.file.clone(), a.start_line, a.end_line);
        let key_b = (b.file.clone(), b.start_line, b.end_line);

        location_to_issues.entry(key_a).or_default().push(idx);
        location_to_issues.entry(key_b).or_default().push(idx);
    }

    // Also track the min instance span per issue for merge filtering.
    let issue_min_span: Vec<usize> = issues
        .iter()
        .map(|issue| {
            let a_span = issue.location_a.end_line - issue.location_a.start_line + 1;
            let b_span = issue.location_b.end_line - issue.location_b.start_line + 1;
            a_span.min(b_span)
        })
        .collect();

    // Union-Find with path compression.
    let n = issues.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    for (_, issue_indices) in &location_to_issues {
        if issue_indices.len() < 2 {
            continue;
        }
        let root = find(&mut parent, issue_indices[0]);
        for &idx in &issue_indices[1..] {
            let other_root = find(&mut parent, idx);
            if root != other_root {
                // Don't merge if the minimum spans differ by more than 10x.
                // This prevents small fragment matches from being merged with
                // huge block-level matches that happen to share a location.
                let root_min = issue_min_span[root];
                let other_min = issue_min_span[other_root];
                let ratio = if root_min < other_min {
                    other_min / root_min.max(1)
                } else {
                    root_min / other_min.max(1)
                };
                if ratio <= 10 {
                    parent[other_root] = root;
                }
            }
        }
    }

    // Collect instances per group (by root).
    let mut group_instances: HashMap<usize, Vec<CloneInstance>> = HashMap::new();

    for (idx, issue) in issues.iter().enumerate() {
        let root = find(&mut parent, idx);
        let instances = group_instances.entry(root).or_default();
        maybe_add_instance(instances, &issue.location_a);
        maybe_add_instance(instances, &issue.location_b);
    }

    // Build groups, filtering out singletons.
    let mut groups: Vec<CloneGroup> = group_instances
        .into_values()
        .filter_map(|instances| {
            if instances.len() < 2 {
                return None;
            }

            let line_count = instances
                .iter()
                .map(|i| i.end_line - i.start_line + 1)
                .min()
                .unwrap_or(0);

            Some(CloneGroup {
                token_count: line_count * TOKENS_PER_LINE,
                line_count,
                instances,
            })
        })
        .collect();

    // Sort by line count descending (biggest duplications first).
    groups.sort_by(|a, b| b.line_count.cmp(&a.line_count));
    groups
}

/// Add an instance to the list only if it's not already there (same file/lines).
fn maybe_add_instance(instances: &mut Vec<CloneInstance>, loc: &CodeBlockLocation) {
    let already_present = instances.iter().any(|i| {
        i.file == loc.file && i.start_line == loc.start_line && i.end_line == loc.end_line
    });
    if !already_present {
        instances.push(CloneInstance {
            file: loc.file.clone(),
            start_line: loc.start_line,
            end_line: loc.end_line,
            snippet: loc.snippet.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CodeBlockLocation;

    fn loc(file: &str, start: usize, end: usize) -> CodeBlockLocation {
        CodeBlockLocation {
            file: file.to_string(),
            name: "fn".to_string(),
            start_line: start,
            end_line: end,
            snippet: "let x = 1;".to_string(),
        }
    }

    fn issue(a: CodeBlockLocation, b: CodeBlockLocation) -> DuplicateCodeIssue {
        DuplicateCodeIssue {
            confidence: 0.95,
            location_a: a,
            location_b: b,
        }
    }

    #[test]
    fn empty_input() {
        let groups = build_clone_groups(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn single_issue_becomes_one_group() {
        let issues = vec![issue(loc("a.ts", 1, 10), loc("b.ts", 5, 14))];
        let groups = build_clone_groups(&issues);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].instances.len(), 2);
        assert_eq!(groups[0].line_count, 10); // 10 - 1 + 1
        assert_eq!(groups[0].token_count, 60);
    }

    #[test]
    fn three_way_merge() {
        // A-B pair and A-C pair with same range in A → merge into 3-instance group.
        let issues = vec![
            issue(loc("a.ts", 1, 5), loc("b.ts", 10, 14)),
            issue(loc("a.ts", 1, 5), loc("c.ts", 20, 24)),
        ];
        let groups = build_clone_groups(&issues);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].instances.len(), 3);
    }

    #[test]
    fn separate_issues_remain_separate() {
        let issues = vec![
            issue(loc("a.ts", 1, 10), loc("b.ts", 1, 10)),
            issue(loc("c.ts", 1, 5), loc("d.ts", 1, 5)),
        ];
        let groups = build_clone_groups(&issues);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn sorted_by_line_count_descending() {
        let issues = vec![
            issue(loc("a.ts", 1, 5), loc("b.ts", 1, 5)),
            issue(loc("c.ts", 1, 20), loc("d.ts", 1, 20)),
        ];
        let groups = build_clone_groups(&issues);
        assert!(groups[0].line_count >= groups[1].line_count);
    }

    #[test]
    fn transitive_merge() {
        // A-B and B-C with shared B location → all merge.
        let issues = vec![
            issue(loc("a.ts", 1, 5), loc("b.ts", 1, 5)),
            issue(loc("b.ts", 1, 5), loc("c.ts", 1, 5)),
        ];
        let groups = build_clone_groups(&issues);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].instances.len(), 3);
    }
}
