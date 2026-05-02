//! Repetitive token pattern detection across files.
//!
//! Finds short code idioms that repeat across many files — e.g.
//! `const * = useState` in 18 files, or `export default function` in 31 files.
//!
//! Approach:
//!   1. Split each file into tokens (word-boundary, language-agnostic)
//!   2. Extract n-grams (2, 3, 4 tokens)
//!   3. Count occurrences across files
//!   4. Weight by information content (rare patterns score higher than `const`)
//!   5. Surface top patterns sorted by (info_score × file_count)
//!
//! No AST, no language-specific logic. Works on any text.

use std::collections::HashMap;

use crate::types::RepetitivePattern;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// N-gram sizes to extract.
const NGRAM_SIZES: &[usize] = &[2, 3, 4];

/// Minimum number of distinct files a pattern must appear in.
const MIN_FILE_COUNT: usize = 3;

/// Maximum patterns to return (top N by score).
const MAX_PATTERNS: usize = 30;

/// Maximum example files to list per pattern.
const MAX_EXAMPLE_FILES: usize = 5;

/// Tokens that carry almost no information (skip for 1-grams, allow in n-grams).
const STOP_TOKENS: &[&str] = &[
    "{", "}", "(", ")", "[", "]", ";", ",", ".", ":", "::", "=>", "->", "=", "==", "===", "!=", "!==", "<", ">", "<=",
    ">=", "+", "-", "*", "/", "|", "||", "&&", "&", "#", "@", "?", "??", "!", "...", "..",
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect repetitive token patterns across files.
///
/// Returns patterns sorted by (info_score × file_count) descending,
/// capped at [`MAX_PATTERNS`].
pub fn detect_patterns(file_sources: &[(String, String)]) -> Vec<RepetitivePattern> {
    if file_sources.len() < MIN_FILE_COUNT {
        return vec![];
    }

    // Tokenize all files.
    let file_tokens: Vec<(&str, Vec<String>)> =
        file_sources.iter().map(|(path, source)| (path.as_str(), tokenize(source))).collect();

    // Collect n-gram stats: (ngram_string) → (total_occurrences, set_of_files).
    let mut ngram_stats: HashMap<String, (usize, Vec<String>)> = HashMap::new();

    for (path, tokens) in &file_tokens {
        for &n in NGRAM_SIZES {
            if tokens.len() < n {
                continue;
            }
            for window in tokens.windows(n) {
                // Skip n-grams that are all stop tokens.
                if window.iter().all(|t| STOP_TOKENS.contains(&t.as_str())) {
                    continue;
                }
                let key = window.join(" ");
                let entry = ngram_stats.entry(key).or_insert((0, Vec::new()));
                entry.0 += 1;
                // Only add file once per n-gram (avoid inflating file_count from repeated uses).
                if entry.1.last() != Some(&path.to_string()) {
                    entry.1.push(path.to_string());
                }
            }
        }
    }

    let total_files = file_sources.len() as f64;

    // Score and rank.
    let mut scored: Vec<RepetitivePattern> = ngram_stats
        .into_iter()
        .filter(|(_, (occurrences, files))| *occurrences >= MIN_FILE_COUNT && files.len() >= MIN_FILE_COUNT)
        .map(|(pattern, (occurrences, files))| {
            let file_count = files.len();
            // Information score: inverse document frequency — rare patterns score high.
            // log2(N / df) where df = files containing this pattern.
            let idf = (total_files / file_count as f64).log2();
            // Also factor in average repetition per file (occurrences / files).
            let avg_per_file = occurrences as f64 / file_count as f64;
            // Combined score: idf captures rarity, avg_per_file captures repetition.
            let info_score = (idf * avg_per_file).clamp(0.0, 10.0) / 10.0;

            // Collect example files (first N, alphabetically sorted).
            let mut example_files = files;
            example_files.sort();
            example_files.truncate(MAX_EXAMPLE_FILES);

            RepetitivePattern {
                pattern,
                file_count,
                occurrences,
                info_score: (info_score * 100.0).round() / 100.0,
                example_files,
            }
        })
        .collect();

    // Sort by (info_score × file_count) descending — high-info patterns in many files first.
    scored.sort_by(|a, b| {
        let sa = a.info_score * a.file_count as f64;
        let sb = b.info_score * b.file_count as f64;
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    scored.truncate(MAX_PATTERNS);

    // Deduplicate: remove patterns that are substrings of a longer pattern
    // with the same file_count (i.e. the longer one is more specific).
    deduplicate_substrings(&mut scored);

    scored
}

// ---------------------------------------------------------------------------
// Tokenization
// ---------------------------------------------------------------------------

/// Split source code into tokens on word boundaries.
///
/// Language-agnostic but code-aware:
/// - `vec!`, `assert!`, `macro!` are kept as single tokens
/// - `->`, `=>`, `::`, `==`, `===` are kept as single tokens
/// - `foo::bar` splits into `foo`, `::`, `bar`
fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Skip whitespace.
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // Alphanumeric/underscore run (identifiers, keywords, numbers).
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let mut tok: String = chars[start..i].iter().collect();
            // Check if followed by `!` (macro: vec!, assert!, println!).
            if i < chars.len() && chars[i] == '!' {
                tok.push('!');
                i += 1;
            }
            tokens.push(tok);
            continue;
        }

        // String literal: "..." — collapse to a single token.
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            if i < chars.len() {
                i += 1; // closing quote
            }
            tokens.push(chars[start..i].iter().collect());
            continue;
        }

        // Multi-char operators.
        let two_char: String = if i + 1 < chars.len() { chars[i..i + 2].iter().collect() } else { String::new() };
        let three_char: String = if i + 2 < chars.len() { chars[i..i + 3].iter().collect() } else { String::new() };

        if ["===", "!==", "..."].contains(&three_char.as_str()) {
            tokens.push(three_char);
            i += 3;
        } else if ["->", "=>", "::", "==", "!=", "&&", "||", "??", "<=", ">=", ".."].contains(&two_char.as_str()) {
            tokens.push(two_char);
            i += 2;
        } else {
            // Single-char punctuation.
            tokens.push(ch.to_string());
            i += 1;
        }
    }

    tokens
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

/// Remove patterns that are substrings of a longer pattern with the same
/// (or very similar) file count. E.g. if "; assert! (" exists, remove
/// "assert! (" and "; assert!" as redundant.
fn deduplicate_substrings(patterns: &mut Vec<RepetitivePattern>) {
    // Keep patterns that are NOT a substring of any longer pattern
    // with >= 80% of the file count.
    let mut to_remove = vec![false; patterns.len()];

    for i in 0..patterns.len() {
        if to_remove[i] {
            continue;
        }
        for j in 0..patterns.len() {
            if i == j || to_remove[j] {
                continue;
            }
            // j is a "competitor" — if pattern[i] is a substring of pattern[j]
            // and pattern[j] is longer, and they share similar file counts, drop i.
            let pi = &patterns[i].pattern;
            let pj = &patterns[j].pattern;

            if pi.len() < pj.len() && pj.contains(pi.as_str()) {
                let file_ratio = patterns[i].file_count as f64 / patterns[j].file_count as f64;
                // If the longer pattern appears in >= 80% of the same files, drop the shorter.
                if file_ratio >= 0.8 {
                    to_remove[i] = true;
                    break;
                }
            }
        }
    }

    // Apply removals.
    let mut i = 0;
    while i < to_remove.len() {
        if to_remove[i] {
            patterns.remove(i);
            to_remove.remove(i);
        } else {
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("const [user, setUser] = useState<User>();");
        assert_eq!(
            tokens,
            vec!["const", "[", "user", ",", "setUser", "]", "=", "useState", "<", "User", ">", "(", ")", ";"]
        );
    }

    #[test]
    fn tokenize_rust() {
        let tokens = tokenize("fn main() -> Result<(), String> {");
        assert_eq!(tokens, vec!["fn", "main", "(", ")", "->", "Result", "<", "(", ")", ",", "String", ">", "{"]);
    }

    #[test]
    fn tokenize_macros() {
        let tokens = tokenize("vec! [1, 2] assert! (x > 0)");
        assert_eq!(tokens, vec!["vec!", "[", "1", ",", "2", "]", "assert!", "(", "x", ">", "0", ")"]);
    }

    #[test]
    fn tokenize_strings() {
        let tokens = tokenize("\"hello world\" x");
        assert_eq!(tokens, vec!["\"hello world\"", "x"]);
    }

    #[test]
    fn detect_patterns_finds_common_idioms() {
        let files = vec![
            ("a.ts".to_string(), "const [x, setX] = useState();\nexport default function App() {}\n".to_string()),
            ("b.ts".to_string(), "const [y, setY] = useState();\nexport default function Page() {}\n".to_string()),
            ("c.ts".to_string(), "const [z, setZ] = useState();\nexport default function Modal() {}\n".to_string()),
            ("d.ts".to_string(), "const [w, setW] = useState();\nexport default function Card() {}\n".to_string()),
        ];
        let patterns = detect_patterns(&files);
        // Should find "const" and "useState" patterns across 4 files.
        assert!(!patterns.is_empty(), "should find some patterns");

        let const_patterns: Vec<_> = patterns.iter().filter(|p| p.pattern.contains("const")).collect();
        assert!(!const_patterns.is_empty(), "should find patterns containing 'const'");

        let use_state: Vec<_> = patterns.iter().filter(|p| p.pattern.contains("useState")).collect();
        assert!(!use_state.is_empty(), "should find patterns containing 'useState'");
    }

    #[test]
    fn too_few_files_returns_empty() {
        let files =
            vec![("a.ts".to_string(), "const x = 1;".to_string()), ("b.ts".to_string(), "const x = 1;".to_string())];
        let patterns = detect_patterns(&files);
        assert!(patterns.is_empty());
    }

    #[test]
    fn max_patterns_respected() {
        let mut files = Vec::new();
        for i in 0..50 {
            files.push((format!("f{i}.ts"), "const x = 1;\nlet y = 2;\n".to_string()));
        }
        let patterns = detect_patterns(&files);
        assert!(patterns.len() <= MAX_PATTERNS);
    }

    #[test]
    fn info_score_rare_beats_common() {
        let files = vec![
            ("a.ts".to_string(), "rareThing();\nconst x = 1;\n".to_string()),
            ("b.ts".to_string(), "rareThing();\nconst x = 2;\n".to_string()),
            ("c.ts".to_string(), "rareThing();\nconst x = 3;\n".to_string()),
            ("d.ts".to_string(), "const x = 4;\n".to_string()),
            ("e.ts".to_string(), "const x = 5;\n".to_string()),
        ];
        let patterns = detect_patterns(&files);
        // "rareThing" is in 3/5 files (rarer), "const" is in 5/5 (common).
        // Both should appear but rareThing should have higher info_score.
        let rare = patterns.iter().find(|p| p.pattern.contains("rareThing"));
        let common = patterns.iter().find(|p| p.pattern.contains("const") && !p.pattern.contains("rareThing"));
        if let (Some(r), Some(c)) = (rare, common) {
            assert!(
                r.info_score >= c.info_score,
                "rare pattern should have >= info_score: {} vs {}",
                r.info_score,
                c.info_score
            );
        }
    }
}
