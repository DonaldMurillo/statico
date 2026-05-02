//! Normalized Compression Distance (NCD) for cheap, language-agnostic duplicate detection.
//!
//! NCD leverages the fact that if two code fragments are similar, compressing
//! them *together* yields a size close to compressing just one — because the
//! compressor naturally finds and exploits the shared structure.
//! ```text
//! NCD(a, b) = (C(a+b) - min(C(a), C(b))) / max(C(a), C(b))
//! ```
//!
//! NCD ≈ 0 → near-identical, ≈ 1 → unrelated.
//!
//! Uses zstd for compression (~1 GB/s, good on structured text).
//! This is intended as a **pre-filter**: cheap O(n²) pairwise comparison
//! to identify candidate file pairs, which are then fed to the expensive
//! AST-based block/fragment detector.

use std::collections::HashMap;

use crate::types::{CodeBlockLocation, DuplicateCodeIssue};

// ---------------------------------------------------------------------------
// Compression helpers
// ---------------------------------------------------------------------------

/// Compress `data` with zstd level 1 (fastest) and return compressed size.
fn compressed_size(data: &[u8]) -> usize {
    // Level 1 is ~400 MB/s and sufficient for NCD — we only need relative sizes.
    zstd::bulk::compress(data, 1).map(|c| c.len()).unwrap_or(data.len())
}

/// Compute NCD between two byte strings.
///
/// Returns a value in [0, 1] where 0 = identical, 1 = unrelated.
/// Values can occasionally be slightly negative or > 1 due to compression
/// artifacts; we clamp to [0, 1].
pub fn ncd(a: &[u8], b: &[u8]) -> f64 {
    let ca = compressed_size(a);
    let cb = compressed_size(b);
    let cab = compressed_size(&[a, b].concat());
    let (min_c, max_c) = if ca < cb { (ca, cb) } else { (cb, ca) };
    if max_c == 0 {
        return 0.0;
    }
    let raw = (cab as f64 - min_c as f64) / max_c as f64;
    raw.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// NCD-based file-level duplicate detection
// ---------------------------------------------------------------------------

/// A file pair flagged as potentially duplicated by NCD.
#[derive(Debug, Clone)]
pub struct NcdCandidate {
    pub path_a: String,
    pub path_b: String,
    pub distance: f64,
}

/// Compare all file pairs using NCD and return pairs below `threshold`.
///
/// Files shorter than `min_lines` are skipped (too short for meaningful NCD).
/// This is O(n²) but each comparison is ~microseconds with zstd level 1.
pub fn find_candidate_pairs(
    file_sources: &[(String, String)],
    threshold: f64,
    min_lines: usize,
) -> Vec<NcdCandidate> {
    // Pre-filter: only consider files with enough content.
    let eligible: Vec<(&str, &[u8])> = file_sources
        .iter()
        .filter(|(_, src)| src.lines().count() >= min_lines)
        .map(|(p, s)| (p.as_str(), s.as_bytes()))
        .collect();

    if eligible.len() < 2 {
        return vec![];
    }

    // Pre-compute individual compressed sizes to avoid redundant work.
    let c_sizes: Vec<usize> = eligible.iter().map(|(_, data)| compressed_size(data)).collect();

    let mut candidates = Vec::new();

    for i in 0..eligible.len() {
        for j in (i + 1)..eligible.len() {
            let (path_a, data_a) = &eligible[i];
            let (path_b, data_b) = &eligible[j];

            // Compute C(a+b) only.
            let combined: Vec<u8> = data_a.iter().chain(data_b.iter()).copied().collect();
            let cab = compressed_size(&combined);

            let (min_c, max_c) = if c_sizes[i] < c_sizes[j] { (c_sizes[i], c_sizes[j]) } else { (c_sizes[j], c_sizes[i]) };

            if max_c == 0 {
                continue;
            }

            let distance = ((cab as f64 - min_c as f64) / max_c as f64).clamp(0.0, 1.0);

            if distance <= threshold {
                candidates.push(NcdCandidate { path_a: path_a.to_string(), path_b: path_b.to_string(), distance });
            }
        }
    }

    // Sort by distance ascending (most similar first).
    candidates.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
    candidates
}

// ---------------------------------------------------------------------------
// Convert NCD candidates to DuplicateCodeIssues (file-level signals)
// ---------------------------------------------------------------------------

/// Threshold below which two files are considered potentially duplicated.
const NCD_DUPLICATE_THRESHOLD: f64 = 0.4;

/// Minimum lines a file must have to participate in NCD comparison.
const NCD_MIN_LINES: usize = 10;

/// Detect file-level duplicate candidates using NCD.
///
/// Returns `DuplicateCodeIssue`s covering the full file content.
/// These are coarse-grained signals — the block/fragment detectors produce
/// the precise line-range matches. The NCD results serve as:
///   1. A fast pre-filter telling the fragment detector which file pairs to focus on
///   2. A complementary "these files smell alike" signal for the report
pub fn detect_ncd_duplicates(file_sources: &[(String, String)]) -> Vec<DuplicateCodeIssue> {
    let candidates = find_candidate_pairs(file_sources, NCD_DUPLICATE_THRESHOLD, NCD_MIN_LINES);

    // Pre-compute line counts for file ranges.
    let line_counts: HashMap<&str, usize> =
        file_sources.iter().map(|(p, s)| (p.as_str(), s.lines().count())).collect();

    candidates
        .into_iter()
        .filter_map(|c| {
            let end_a = *line_counts.get(c.path_a.as_str())?;
            let end_b = *line_counts.get(c.path_b.as_str())?;
            if end_a == 0 || end_b == 0 {
                return None;
            }
            // Convert NCD distance to confidence: distance 0 → conf 1.0, distance threshold → conf ~0.5
            let confidence = 1.0 - c.distance;
            Some(DuplicateCodeIssue {
                confidence: (confidence * 100.0).round() / 100.0,
                location_a: CodeBlockLocation {
                    file: c.path_a,
                    name: "ncd-file".to_string(),
                    start_line: 1,
                    end_line: end_a,
                    snippet: String::new(),
                },
                location_b: CodeBlockLocation {
                    file: c.path_b,
                    name: "ncd-file".to_string(),
                    start_line: 1,
                    end_line: end_b,
                    snippet: String::new(),
                },
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_files_have_zero_ncd() {
        // Need enough content that compression overhead doesn't dominate.
        let code = "fn main() { println!(\"hello\"); }\n".repeat(10);
        let d = ncd(code.as_bytes(), code.as_bytes());
        assert!(d < 0.05, "identical files should have NCD < 0.05, got {d}");
    }

    #[test]
    fn unrelated_files_have_high_ncd() {
        let a = b"fn main() { println!(\"hello world\"); }\n";
        let b = b"use std::collections::HashMap;\nstruct Foo { x: i32, y: String }\n";
        let d = ncd(a, b);
        assert!(d > 0.3, "unrelated files should have NCD > 0.3, got {d}");
    }

    #[test]
    fn slightly_modified_has_low_ncd() {
        let a = b"fn foo(x: i32) -> i32 {\n    x + 1\n}\nfn bar(y: i32) -> i32 {\n    y * 2\n}\n";
        let b = b"fn foo(x: i32) -> i32 {\n    x + 2\n}\nfn bar(y: i32) -> i32 {\n    y * 3\n}\n";
        let d = ncd(a, b);
        assert!(d < 0.3, "slightly modified copy should have NCD < 0.3, got {d}");
    }

    #[test]
    fn find_candidates_filters_by_threshold() {
        let files = vec![
            ("a.rs".to_string(), "fn main() { println!(\"hello\"); }\n".repeat(5)),
            ("b.rs".to_string(), "fn main() { println!(\"hello\"); }\n".repeat(5)), // identical to a
            ("c.rs".to_string(), "struct Point { x: f64, y: f64, z: f64 }\n".repeat(5)), // unrelated
        ];
        let candidates = find_candidate_pairs(&files, 0.5, 3);
        // a-b should be flagged (identical), a-c and b-c should not.
        assert!(candidates.iter().any(|c| c.path_a == "a.rs" && c.path_b == "b.rs"),
            "identical pair should be flagged");
        assert!(!candidates.iter().any(|c|
            (c.path_a == "a.rs" || c.path_a == "b.rs") && c.path_b == "c.rs"),
            "unrelated pairs should not be flagged");
    }

    #[test]
    fn detect_ncd_duplicates_basic() {
        let files = vec![
            ("a.rs".to_string(), "fn main() { println!(\"hello\"); }\n".repeat(10)),
            ("b.rs".to_string(), "fn main() { println!(\"hello\"); }\n".repeat(10)),
            ("c.rs".to_string(), "struct Point { x: f64, y: f64, z: f64 }\n".repeat(10)),
        ];
        let issues = detect_ncd_duplicates(&files);
        // Should find exactly one pair: a-b
        assert_eq!(issues.len(), 1, "expected exactly 1 NCD duplicate pair");
        assert_eq!(issues[0].location_a.file, "a.rs");
        assert_eq!(issues[0].location_b.file, "b.rs");
        assert!(issues[0].confidence > 0.5);
    }

    #[test]
    fn empty_input_returns_empty() {
        let issues = detect_ncd_duplicates(&[]);
        assert!(issues.is_empty());
    }

    #[test]
    fn short_files_skipped() {
        let files = vec![
            ("a.rs".to_string(), "x\n".to_string()),  // 1 line, below min
            ("b.rs".to_string(), "x\n".to_string()),
        ];
        let issues = detect_ncd_duplicates(&files);
        assert!(issues.is_empty());
    }
}
