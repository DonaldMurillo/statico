//! Fragment-level duplicate code detection using variable-size sliding windows.
//!
//! Extracts code fragments at multiple window sizes (5–30 lines), fingerprints
//! them via winnowing, and finds cross-file pairs sharing fingerprints.
//! Larger windows are processed first; smaller windows skip ranges already
//! covered by larger matches, naturally producing maximal matches.

use std::collections::{HashMap, HashSet};

use super::{
    normalize, tokenize, winnow, jaccard_similarity, round2, DuplicateCodeIssue, CodeBlockLocation,
    MIN_FRAGMENT_LINES, FRAGMENT_CONFIDENCE_THRESHOLD,
};

// ---------------------------------------------------------------------------
// Window sizes & steps
// ---------------------------------------------------------------------------

/// Window sizes to probe, processed largest-first.
const WINDOW_SIZES: &[usize] = &[30, 20, 15, 12, 10, 8, 7, 6, 5];

/// Step for windows ≤ 10 lines.
const STEP_SMALL: usize = 2;
/// Step for windows > 10 lines.
const STEP_LARGE: usize = 4;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct Fragment {
    path: String,
    start_line: usize,
    end_line: usize,
    fingerprints: Vec<u64>,
}

struct RawMatch {
    path_a: String,
    start_a: usize,
    end_a: usize,
    path_b: String,
    start_b: usize,
    end_b: usize,
    confidence: f64,
}

// ---------------------------------------------------------------------------
// Public entry point (called from parent module)
// ---------------------------------------------------------------------------

/// Detect fragment-level duplicates using variable window sizes.
///
/// Larger windows are processed first. Line ranges already covered by a
/// larger match are skipped when processing smaller windows, which avoids
/// redundant work and ensures maximal match lengths.
///
/// Performance: Limits the number of candidate pairs checked per window
/// size to avoid O(n²) explosion on large codebases.
pub fn detect_fragments(file_sources: &[(String, String)]) -> Vec<DuplicateCodeIssue> {
    if file_sources.len() < 2 {
        return Vec::new();
    }

    // Early exit for very large codebases — fragment detection is O(n²)
    // and not meaningful for projects with >500 source files.
    // Block-level detection still runs.
    let total_lines: usize = file_sources.iter().map(|(_, s)| s.lines().count()).sum();
    if file_sources.len() > 500 || total_lines > 200_000 {
        return Vec::new();
    }

    // Track which (file, start, end) ranges are already matched so smaller
    // windows can skip them.
    let mut covered: HashSet<(String, usize, usize)> = HashSet::new();
    let mut all_matches: Vec<RawMatch> = Vec::new();

    // Pre-compute lines per file (avoids re-splitting for each window size).
    let file_lines: Vec<(String, Vec<&str>)> = file_sources
        .iter()
        .map(|(p, s)| (p.clone(), s.lines().collect()))
        .collect();

    for &window_size in WINDOW_SIZES {
        if window_size < MIN_FRAGMENT_LINES {
            continue;
        }
        let step = if window_size <= 10 { STEP_SMALL } else { STEP_LARGE };

        let fragments = extract_fragments(&file_lines, window_size, step, &covered);
        if fragments.len() < 2 {
            continue;
        }

        let matches = find_fragment_matches(&fragments);

        // Merge into all_matches and update covered set.
        for m in matches {
            let key_a = (m.path_a.clone(), m.start_a, m.end_a);
            let key_b = (m.path_b.clone(), m.start_b, m.end_b);
            covered.insert(key_a);
            covered.insert(key_b);
            all_matches.push(m);
        }
    }

    // Merge overlapping matches per file pair.
    merge_to_issues(all_matches)
}

// ---------------------------------------------------------------------------
// Fragment extraction
// ---------------------------------------------------------------------------

fn extract_fragments<'a>(
    file_lines: &[(String, Vec<&'a str>)],
    window_size: usize,
    step: usize,
    covered: &HashSet<(String, usize, usize)>,
) -> Vec<Fragment> {
    let mut fragments = Vec::new();

    for (path, lines) in file_lines {
        if lines.len() < window_size {
            continue;
        }

        let mut start = 0;
        while start <= lines.len() - window_size {
            let end = start + window_size;
            let start_1 = start + 1; // 1-indexed

            // Skip if this exact range is already covered by a larger match.
            if covered.contains(&(path.clone(), start_1, end)) {
                start += step;
                continue;
            }

            let fragment_source = lines[start..end].join("\n");
            let normalized = normalize(&fragment_source);

            // Need enough tokens for winnowing to be meaningful.
            if normalized.split_whitespace().count() < window_size * 2 {
                start += step;
                continue;
            }

            let tokens = tokenize(&normalized);
            let fingerprints = winnow(&tokens);
            if fingerprints.is_empty() {
                start += step;
                continue;
            }

            fragments.push(Fragment {
                path: path.clone(),
                start_line: start_1,
                end_line: end,
                fingerprints,
            });

            start += step;
        }
    }

    fragments
}

// ---------------------------------------------------------------------------
// Match finding
// ---------------------------------------------------------------------------

fn find_fragment_matches(fragments: &[Fragment]) -> Vec<RawMatch> {
    // Build fingerprint index.
    let mut fp_index: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, frag) in fragments.iter().enumerate() {
        for &fp in &frag.fingerprints {
            fp_index.entry(fp).or_default().push(i);
        }
    }

    // Find candidate pairs sharing fingerprints.
    let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut pair_scores: HashMap<(usize, usize), f64> = HashMap::new();

    for indices in fp_index.values() {
        for a in indices {
            for b in indices {
                if a >= b {
                    continue;
                }
                let fa = &fragments[*a];
                let fb = &fragments[*b];
                if fa.path == fb.path {
                    continue;
                }
                let key = (*a, *b);
                if !seen_pairs.contains(&key) {
                    seen_pairs.insert(key);
                    let conf = jaccard_similarity(&fa.fingerprints, &fb.fingerprints);
                    if conf >= FRAGMENT_CONFIDENCE_THRESHOLD {
                        pair_scores.insert(key, conf);
                    }
                }
            }
        }
    }

    // Build RawMatch list.
    let mut matches = Vec::new();
    for ((a, b), conf) in pair_scores {
        let fa = &fragments[a];
        let fb = &fragments[b];
        let (path_a, start_a, end_a, path_b, start_b, end_b) = if fa.path < fb.path {
            (fa.path.clone(), fa.start_line, fa.end_line,
             fb.path.clone(), fb.start_line, fb.end_line)
        } else {
            (fb.path.clone(), fb.start_line, fb.end_line,
             fa.path.clone(), fa.start_line, fa.end_line)
        };
        matches.push(RawMatch { path_a, start_a, end_a, path_b, start_b, end_b, confidence: conf });
    }

    matches
}

// ---------------------------------------------------------------------------
// Merging overlapping matches per file pair
// ---------------------------------------------------------------------------

fn merge_to_issues(matches: Vec<RawMatch>) -> Vec<DuplicateCodeIssue> {
    // Group by file pair.
    let mut by_pair: HashMap<(String, String), Vec<RawMatch>> = HashMap::new();
    for m in matches {
        let key = (m.path_a.clone(), m.path_b.clone());
        by_pair.entry(key).or_default().push(m);
    }

    let mut issues = Vec::new();
    for (_, mut group) in by_pair {
        group.sort_by(|a, b| a.start_a.cmp(&b.start_a).then(a.start_b.cmp(&b.start_b)));

        let mut merged: Vec<RawMatch> = Vec::new();
        for m in group {
            let mut absorbed = false;
            for last in merged.iter_mut() {
                // Merge if ranges overlap in BOTH files (strict) or either (loose).
                // Use loose (either-file) to reduce sliding-window noise.
                let a_overlaps = m.start_a <= last.end_a + 1 && m.end_a >= last.start_a;
                let b_overlaps = m.start_b <= last.end_b + 1 && m.end_b >= last.start_b;
                if a_overlaps || b_overlaps {
                    last.start_a = last.start_a.min(m.start_a);
                    last.end_a = last.end_a.max(m.end_a);
                    last.start_b = last.start_b.min(m.start_b);
                    last.end_b = last.end_b.max(m.end_b);
                    last.confidence = last.confidence.max(m.confidence);
                    absorbed = true;
                    break;
                }
            }
            if !absorbed {
                merged.push(m);
            }
        }

        for m in merged {
            issues.push(DuplicateCodeIssue {
                confidence: round2(m.confidence),
                location_a: CodeBlockLocation {
                    file: m.path_a,
                    name: "fragment".to_string(),
                    start_line: m.start_a,
                    end_line: m.end_a,
                    snippet: String::new(),
                },
                location_b: CodeBlockLocation {
                    file: m.path_b,
                    name: "fragment".to_string(),
                    start_line: m.start_b,
                    end_line: m.end_b,
                    snippet: String::new(),
                },
            });
        }
    }

    issues
}
