//! Detect mirrored directories: directory pairs with significant file overlap.

use std::collections::HashMap;

use crate::types::{CloneGroup, MirroredDirectory};

/// Minimum number of shared files to consider directories mirrored.
const MIRROR_THRESHOLD: usize = 3;

/// Detect mirrored directories from clone groups.
///
/// For each group, we look at the directory of each instance's file.
/// We track pairs of directories and the shared file names.
/// Pairs with `MIRROR_THRESHOLD`+ shared files are reported.
pub fn detect_mirrored_directories(groups: &[CloneGroup]) -> Vec<MirroredDirectory> {
    if groups.is_empty() {
        return vec![];
    }

    // Map: (sorted dir_a, sorted dir_b) -> set of shared filenames.
    let mut pair_files: HashMap<(String, String), Vec<String>> = HashMap::new();
    // Map: (sorted dir_a, sorted dir_b) -> total duplicated lines.
    let mut pair_lines: HashMap<(String, String), usize> = HashMap::new();

    for group in groups {
        // Collect unique directories from instances.
        let mut dir_instances: HashMap<String, Vec<&str>> = HashMap::new();
        for inst in &group.instances {
            let dir = parent_dir(&inst.file);
            let filename = filename_only(&inst.file);
            dir_instances
                .entry(dir)
                .or_default()
                .push(filename);
        }

        // For each unique pair of directories, record shared files.
        let dirs: Vec<String> = dir_instances.keys().cloned().collect();
        for i in 0..dirs.len() {
            for j in (i + 1)..dirs.len() {
                let mut pair = (dirs[i].clone(), dirs[j].clone());
                if pair.0 > pair.1 {
                    std::mem::swap(&mut pair.0, &mut pair.1);
                }

                let files_i = &dir_instances[&pair.0];
                let files_j = &dir_instances[&pair.1];

                // Add files from both sides that appear in instances.
                let shared = files_i
                    .iter()
                    .chain(files_j.iter())
                    .map(|s| s.to_string());

                let entry = pair_files.entry(pair.clone()).or_default();
                for f in shared {
                    if !entry.contains(&f) {
                        entry.push(f);
                    }
                }

                *pair_lines.entry(pair.clone()).or_insert(0) += group.line_count;
            }
        }
    }

    let mut mirrors: Vec<MirroredDirectory> = pair_files
        .into_iter()
        .filter_map(|((dir_a, dir_b), shared_files)| {
            if shared_files.len() < MIRROR_THRESHOLD {
                return None;
            }
            let total_lines = *pair_lines.get(&(dir_a.clone(), dir_b.clone())).unwrap_or(&0);
            Some(MirroredDirectory {
                dir_a,
                dir_b,
                shared_files,
                total_lines,
            })
        })
        .collect();

    // Sort by total_lines descending.
    mirrors.sort_by(|a, b| b.total_lines.cmp(&a.total_lines));
    mirrors
}

/// Extract the parent directory from a file path (using `/` separators).
fn parent_dir(path: &str) -> String {
    if let Some(idx) = path.rfind('/') {
        path[..idx].to_string()
    } else {
        ".".to_string()
    }
}

/// Extract just the filename (last component) from a path.
fn filename_only(path: &str) -> &str {
    if let Some(idx) = path.rfind('/') {
        &path[idx + 1..]
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CloneInstance;

    fn instance(file: &str, start: usize, end: usize) -> CloneInstance {
        CloneInstance {
            file: file.to_string(),
            start_line: start,
            end_line: end,
            snippet: String::new(),
        }
    }

    #[test]
    fn empty_input() {
        assert!(detect_mirrored_directories(&[]).is_empty());
    }

    #[test]
    fn below_threshold_not_reported() {
        // Only 2 shared files, below threshold of 3.
        let groups = vec![CloneGroup {
            instances: vec![
                instance("src/a.ts", 1, 10),
                instance("lib/a.ts", 1, 10),
            ],
            token_count: 60,
            line_count: 10,
        }];
        let mirrors = detect_mirrored_directories(&groups);
        assert!(mirrors.is_empty());
    }

    #[test]
    fn above_threshold_reported() {
        let groups = vec![
            CloneGroup {
                instances: vec![
                    instance("src/a.ts", 1, 10),
                    instance("lib/a.ts", 1, 10),
                ],
                token_count: 60,
                line_count: 10,
            },
            CloneGroup {
                instances: vec![
                    instance("src/b.ts", 1, 10),
                    instance("lib/b.ts", 1, 10),
                ],
                token_count: 60,
                line_count: 10,
            },
            CloneGroup {
                instances: vec![
                    instance("src/c.ts", 1, 10),
                    instance("lib/c.ts", 1, 10),
                ],
                token_count: 60,
                line_count: 10,
            },
        ];
        let mirrors = detect_mirrored_directories(&groups);
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0].shared_files.len(), 3);
    }

    #[test]
    fn parent_dir_extraction() {
        assert_eq!(parent_dir("src/foo/bar.ts"), "src/foo");
        assert_eq!(parent_dir("bar.ts"), ".");
    }

    #[test]
    fn filename_extraction() {
        assert_eq!(filename_only("src/foo/bar.ts"), "bar.ts");
        assert_eq!(filename_only("bar.ts"), "bar.ts");
    }
}
