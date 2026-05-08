//! Duplicate code detection using token-based fingerprinting with winnowing.
//!
//! Two detection strategies run in parallel:
//!
//! 1. **Block-level**: Extract named code blocks (functions, methods, arrow fns)
//!    and fingerprint them. Finds whole-function duplicates.
//!
//! 2. **Fragment-level**: Sliding window at multiple sizes over each file's source
//!    lines. Finds sub-function duplicated code of any length ≥ 5 lines.
//!    Delegated to the `fragments` submodule.
//!
//! Results are merged: block-level matches suppress overlapping fragment matches.

mod fragments;

use std::collections::{BTreeMap, HashSet};

use crate::parse::blocks::CodeBlock;
use crate::types::{CodeBlockLocation, DuplicateCodeIssue};

/// Minimum confidence (0.0–1.0) to include in the report (block-level).
const CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Minimum confidence for fragment matches (higher = less noise).
const FRAGMENT_CONFIDENCE_THRESHOLD: f64 = 0.85;

/// K-gram size: number of tokens per sliding window for hashing.
const KGRAM_SIZE: usize = 8;

/// Winnowing window size: pick min hash per this many k-grams.
const WINNOW_WINDOW: usize = 4;

/// Minimum lines a code block must have to be considered.
const MIN_BLOCK_LINES: usize = 5;

/// Minimum lines for a fragment window (shared with fragment module).
const MIN_FRAGMENT_LINES: usize = 5;

/// Maximum snippet length to include in the report.
const MAX_SNIPPET_LEN: usize = 200;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect duplicate code: both block-level and fragment-level.
pub fn detect(
    file_blocks: &BTreeMap<String, Vec<CodeBlock>>,
    file_sources: &[(String, String)],
) -> Vec<DuplicateCodeIssue> {
    let mut issues = Vec::new();

    // Strategy 1: Block-level duplicates.
    let block_issues = detect_blocks(file_blocks);
    issues.extend(block_issues);

    // Strategy 2: Fragment-level duplicates (variable-size sliding window).
    let fragment_issues = fragments::detect_fragments(file_sources);

    // Merge: keep fragment matches that aren't already covered by block matches.
    let block_ranges = build_range_set(&issues);
    for frag in fragment_issues {
        if !is_covered_by_block(&frag, &block_ranges) {
            issues.push(frag);
        }
    }

    // Sort by confidence descending.
    issues.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    issues
}

// ---------------------------------------------------------------------------
// Strategy 1: Block-level detection
// ---------------------------------------------------------------------------

fn detect_blocks(file_blocks: &BTreeMap<String, Vec<CodeBlock>>) -> Vec<DuplicateCodeIssue> {
    let candidates: Vec<(&str, &CodeBlock, Vec<u64>)> = file_blocks
        .iter()
        .flat_map(|(path, blocks)| {
            blocks.iter().filter(|b| b.end_line - b.start_line + 1 >= MIN_BLOCK_LINES).map(|b| {
                let normalized = normalize(&b.source);
                let tokens = tokenize(&normalized);
                let fingerprints = winnow(&tokens);
                (path.as_str(), b, fingerprints)
            })
        })
        .collect();

    find_block_pairs(&candidates)
}

fn find_block_pairs(candidates: &[(&str, &CodeBlock, Vec<u64>)]) -> Vec<DuplicateCodeIssue> {
    let mut fp_index: std::collections::HashMap<u64, Vec<usize>> = std::collections::HashMap::new();
    for (i, (_, _, fps)) in candidates.iter().enumerate() {
        for &fp in fps {
            fp_index.entry(fp).or_default().push(i);
        }
    }

    let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut pairs_to_check: Vec<(usize, usize)> = Vec::new();
    for indices in fp_index.values() {
        for a in indices {
            for b in indices {
                if a < b && !seen_pairs.contains(&(*a, *b)) {
                    seen_pairs.insert((*a, *b));
                    pairs_to_check.push((*a, *b));
                }
            }
        }
    }

    let mut issues = Vec::new();
    for (i, j) in pairs_to_check {
        let (path_a, block_a, fps_a) = &candidates[i];
        let (path_b, block_b, fps_b) = &candidates[j];
        if *path_a == *path_b || fps_a.is_empty() || fps_b.is_empty() {
            continue;
        }
        let confidence = jaccard_similarity(fps_a, fps_b);
        if confidence >= CONFIDENCE_THRESHOLD {
            issues.push(DuplicateCodeIssue {
                confidence: round2(confidence),
                location_a: make_location(path_a, &block_a.name, block_a.start_line, block_a.end_line, &block_a.source),
                location_b: make_location(path_b, &block_b.name, block_b.start_line, block_b.end_line, &block_b.source),
            });
        }
    }
    issues
}

// ---------------------------------------------------------------------------
// Dedup: suppress fragments covered by block matches
// ---------------------------------------------------------------------------

fn build_range_set(issues: &[DuplicateCodeIssue]) -> HashSet<(String, usize, usize)> {
    let mut set = HashSet::new();
    for issue in issues {
        set.insert((issue.location_a.file.clone(), issue.location_a.start_line, issue.location_a.end_line));
        set.insert((issue.location_b.file.clone(), issue.location_b.start_line, issue.location_b.end_line));
    }
    set
}

fn is_covered_by_block(frag: &DuplicateCodeIssue, block_ranges: &HashSet<(String, usize, usize)>) -> bool {
    let a_covered = block_ranges.iter().any(|(file, start, end)| {
        *file == frag.location_a.file && *start <= frag.location_a.start_line && *end >= frag.location_a.end_line
    });
    let b_covered = block_ranges.iter().any(|(file, start, end)| {
        *file == frag.location_b.file && *start <= frag.location_b.start_line && *end >= frag.location_b.end_line
    });
    a_covered && b_covered
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn normalize(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev_ws = false;
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                result.push(' ');
                prev_ws = true;
            }
            continue;
        }
        if ch == '/' {
            match chars.peek() {
                Some(&'/') => {
                    chars.next();
                    in_line_comment = true;
                    continue;
                }
                Some(&'*') => {
                    chars.next();
                    in_block_comment = true;
                    continue;
                }
                _ => {}
            }
        }
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
                prev_ws = true;
            }
            continue;
        }
        prev_ws = false;
        result.push(ch.to_ascii_lowercase());
    }
    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tokenization & fingerprinting
// ---------------------------------------------------------------------------

fn tokenize(normalized: &str) -> Vec<&str> {
    normalized.split_whitespace().collect()
}

fn kgram_hashes(tokens: &[&str], k: usize) -> Vec<u64> {
    if tokens.len() < k {
        return vec![];
    }
    (0..=tokens.len() - k)
        .map(|i| {
            let mut h: u64 = 0;
            for j in 0..k {
                h = h.wrapping_mul(31).wrapping_add(hash_token(tokens[i + j]));
            }
            h
        })
        .collect()
}

fn hash_token(token: &str) -> u64 {
    let mut h: u64 = 0x811c9dc5;
    for byte in token.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

fn winnow(tokens: &[&str]) -> Vec<u64> {
    let kgrams = kgram_hashes(tokens, KGRAM_SIZE);
    if kgrams.is_empty() {
        return vec![];
    }
    let w = WINNOW_WINDOW.min(kgrams.len());
    let mut selected: Vec<u64> = Vec::new();
    for window in kgrams.windows(w) {
        let min_hash = *window.iter().min().unwrap_or(&0);
        if selected.last() != Some(&min_hash) {
            selected.push(min_hash);
        }
    }
    selected.sort();
    selected.dedup();
    selected
}

// ---------------------------------------------------------------------------
// Similarity
// ---------------------------------------------------------------------------

fn jaccard_similarity(a: &[u64], b: &[u64]) -> f64 {
    let set_a: HashSet<u64> = a.iter().copied().collect();
    let set_b: HashSet<u64> = b.iter().copied().collect();
    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 { 0.0 } else { intersection / union }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_location(path: &str, name: &str, start: usize, end: usize, source: &str) -> CodeBlockLocation {
    CodeBlockLocation {
        file: path.to_string(),
        name: name.to_string(),
        start_line: start,
        end_line: end,
        snippet: truncate(source),
    }
}

fn truncate(s: &str) -> String {
    if s.len() > MAX_SNIPPET_LEN {
        // Find a valid char boundary to avoid splitting multi-byte UTF-8.
        let end = s.floor_char_boundary(MAX_SNIPPET_LEN);
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::blocks::{BlockKind, CodeBlock};

    #[test]
    fn normalize_strips_comments_and_whitespace() {
        let src = "function foo() {\n  // comment\n  const x = 1;\n  /* block */\n  return x;\n}";
        let norm = normalize(src);
        assert!(!norm.contains("comment"));
        assert!(!norm.contains("block"));
    }

    #[test]
    fn tokenize_splits_on_whitespace() {
        assert_eq!(tokenize("const x = 1;"), vec!["const", "x", "=", "1;"]);
    }

    #[test]
    fn jaccard_identical_sets() {
        let a = vec![1, 2, 3, 4, 5];
        assert!((jaccard_similarity(&a, &a) - 1.0).abs() < 0.001);
    }

    #[test]
    fn jaccard_disjoint_sets() {
        assert!((jaccard_similarity(&[1, 2, 3], &[4, 5, 6])).abs() < 0.001);
    }

    #[test]
    fn kgram_hashes_correct_count() {
        let tokens = vec!["a", "b", "c", "d", "e"];
        assert_eq!(kgram_hashes(&tokens, 3).len(), 3);
    }

    #[test]
    fn winnow_produces_fingerprints() {
        let tokens = vec!["const", "x", "=", "1;", "return", "x", "+", "1;", "const", "y", "=", "2;"];
        let fps = winnow(&tokens);
        assert!(!fps.is_empty());
        for i in 1..fps.len() {
            assert!(fps[i] >= fps[i - 1]);
        }
    }

    #[test]
    fn jaccard_partial_overlap() {
        let sim = jaccard_similarity(&[1, 2, 3, 4], &[3, 4, 5, 6]);
        assert!((sim - 0.333).abs() < 0.01);
    }

    #[test]
    fn detect_skips_below_threshold() {
        let block_a = CodeBlock {
            name: "alpha".into(),
            source: "function alpha() {\n  const a = 1;\n  const b = 2;\n  const c = 3;\n  return a + b + c;\n}".into(),
            start_line: 1,
            end_line: 6,
            kind: BlockKind::Function,
        };
        let block_b = CodeBlock {
            name: "beta".into(),
            source: "function beta() {\n  const x = 10;\n  const y = 20;\n  const z = 30;\n  return x * y * z;\n}"
                .into(),
            start_line: 1,
            end_line: 6,
            kind: BlockKind::Function,
        };
        let file_blocks = BTreeMap::from([("src/a.ts".into(), vec![block_a]), ("src/b.ts".into(), vec![block_b])]);
        for issue in detect(&file_blocks, &[]) {
            assert!(issue.confidence >= CONFIDENCE_THRESHOLD);
        }
    }

    #[test]
    fn detect_finds_identical_blocks() {
        let code = "function processUser(user: User) {\n  const name = user.name;\n  const email = user.email;\n  const age = user.age;\n  console.log(name, email, age);\n  return { name, email, age };\n}";
        let block = CodeBlock {
            name: "processUser".into(),
            source: code.into(),
            start_line: 1,
            end_line: 7,
            kind: BlockKind::Function,
        };
        let file_blocks = BTreeMap::from([("src/a.ts".into(), vec![block.clone()]), ("src/b.ts".into(), vec![block])]);
        let issues = detect(&file_blocks, &[]);
        assert!(issues.iter().any(|i| i.confidence >= 0.9), "expected high-confidence match");
    }

    #[test]
    fn detect_finds_fragment_duplicates() {
        // Identical code in both files — not inside any function block.
        let code = "const x = getData();\nconst y = transform(x);\nconst z = validate(y);\nconst w = format(z);\nconst v = output(w);\nconsole.log(v);\n";
        let file_a = format!("// file a\n{code}// end");
        let file_b = format!("// file b\n{code}// end");
        let sources = vec![("src/a.ts".to_string(), file_a), ("src/b.ts".to_string(), file_b)];
        let issues = detect(&BTreeMap::new(), &sources);
        assert!(!issues.is_empty(), "expected fragment duplicate, got {} issues", issues.len());
    }

    #[test]
    fn fragments_find_longer_duplicates() {
        // 12 identical lines — old single-window approach (size=5) would only
        // find 5-line fragments. Variable windows should produce a single
        // larger match.
        let line = "  const value = computeSomething(data, opts);\n";
        let code = line.repeat(12);
        let sources = vec![("src/a.ts".to_string(), code.clone()), ("src/b.ts".to_string(), code)];
        let issues = detect(&BTreeMap::new(), &sources);
        assert!(!issues.is_empty(), "expected fragment match");
        // The largest match should span more than 5 lines.
        let max_span = issues
            .iter()
            .map(|i| {
                (i.location_a.end_line - i.location_a.start_line + 1)
                    .max(i.location_b.end_line - i.location_b.start_line + 1)
            })
            .max()
            .unwrap_or(0);
        assert!(max_span > 5, "expected a match spanning >5 lines, got max span {max_span}");
    }
}
