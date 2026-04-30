//! Build clone families: groups of clone groups involving overlapping file sets.
//!
//! A family is defined by a pair of files that share 2+ clone groups. This is
//! broader than exact-file-set matching and aligns with fallow's approach, which
//! groups by shared code patterns between file pairs.

use std::collections::HashMap;

use crate::types::{CloneFamily, CloneGroup};

/// Build clone families from clone groups.
///
/// For each clone group, generate all file pairs from its instances. Group by
/// file pair. Each pair with 2+ shared clone groups becomes a family.
pub fn build_clone_families(groups: &[CloneGroup]) -> Vec<CloneFamily> {
    if groups.is_empty() {
        return vec![];
    }

    // Map: (file_a, file_b) sorted pair → list of clone group indices.
    let mut pair_groups: HashMap<(String, String), Vec<usize>> = HashMap::new();

    for (gi, group) in groups.iter().enumerate() {
        // Collect unique files in this group.
        let mut files: Vec<&str> = group.instances.iter().map(|i| i.file.as_str()).collect();
        files.sort();
        files.dedup();

        // Generate all sorted file pairs.
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let key = (files[i].to_string(), files[j].to_string());
                pair_groups.entry(key).or_default().push(gi);
            }
        }
    }

    // Build families from pairs with 2+ groups.
    let mut families: Vec<CloneFamily> = pair_groups
        .into_iter()
        .filter_map(|((fa, fb), group_indices)| {
            if group_indices.len() < 2 {
                return None;
            }
            let total_duplicated_lines: usize = group_indices.iter().map(|&gi| groups[gi].line_count).sum();
            Some(CloneFamily { files: vec![fa, fb], group_count: group_indices.len(), total_duplicated_lines })
        })
        .collect();

    // Sort by total_duplicated_lines descending.
    families.sort_by(|a, b| b.total_duplicated_lines.cmp(&a.total_duplicated_lines));
    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CloneInstance;

    fn group(files: &[(&str, usize, usize)], line_count: usize) -> CloneGroup {
        CloneGroup {
            instances: files
                .iter()
                .map(|(f, s, e)| CloneInstance {
                    file: f.to_string(),
                    start_line: *s,
                    end_line: *e,
                    snippet: String::new(),
                })
                .collect(),
            token_count: line_count * 6,
            line_count,
        }
    }

    #[test]
    fn empty_input() {
        assert!(build_clone_families(&[]).is_empty());
    }

    #[test]
    fn single_group_no_family() {
        // A single group can't form a family (need 2+ groups per pair).
        let groups = vec![group(&[("a.ts", 1, 10), ("b.ts", 1, 10)], 10)];
        let families = build_clone_families(&groups);
        assert!(families.is_empty());
    }

    #[test]
    fn two_groups_same_file_pair_form_family() {
        let groups =
            vec![group(&[("a.ts", 1, 10), ("b.ts", 1, 10)], 10), group(&[("a.ts", 20, 30), ("b.ts", 20, 30)], 11)];
        let families = build_clone_families(&groups);
        assert_eq!(families.len(), 1);
        assert_eq!(families[0].group_count, 2);
        assert_eq!(families[0].total_duplicated_lines, 21);
        assert_eq!(families[0].files, vec!["a.ts", "b.ts"]);
    }

    #[test]
    fn different_pairs_separate_families() {
        let groups = vec![
            group(&[("a.ts", 1, 10), ("b.ts", 1, 10)], 10),
            group(&[("a.ts", 1, 10), ("b.ts", 1, 10)], 10),
            group(&[("c.ts", 1, 10), ("d.ts", 1, 10)], 10),
            group(&[("c.ts", 1, 10), ("d.ts", 1, 10)], 10),
        ];
        let families = build_clone_families(&groups);
        assert_eq!(families.len(), 2);
    }

    #[test]
    fn three_files_create_multiple_pairs() {
        // A group with 3 files creates pairs (A,B), (A,C), (B,C).
        // Each pair has only 1 group, so no families.
        let groups = vec![group(&[("a.ts", 1, 10), ("b.ts", 1, 10), ("c.ts", 1, 10)], 10)];
        let families = build_clone_families(&groups);
        assert!(families.is_empty());
    }

    #[test]
    fn multi_file_group_with_repeated_pair() {
        // Two groups both involving a.ts, b.ts, c.ts.
        // Pairs: (a,b)→2 groups, (a,c)→2 groups, (b,c)→2 groups = 3 families.
        let groups = vec![
            group(&[("a.ts", 1, 10), ("b.ts", 1, 10), ("c.ts", 1, 10)], 10),
            group(&[("a.ts", 20, 30), ("b.ts", 20, 30), ("c.ts", 20, 30)], 11),
        ];
        let families = build_clone_families(&groups);
        assert_eq!(families.len(), 3);
        // All should have group_count = 2.
        assert!(families.iter().all(|f| f.group_count == 2));
    }
}
