//! AST-based gotcha detection.
//!
//! Detects empty catches, unhandled promises, high complexity, and callback hell.

use crate::parse::AstParser;
use crate::parse::blocks::CodeBlock;
use crate::types::GotchaIssue;

use super::{is_comment_line, is_test_file, truncate_line};

/// Run all AST-based gotcha checks on a single file.
pub fn detect_ast_gotchas(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>) {
    let parser = match AstParser::new() {
        Ok(p) => p,
        Err(_) => return,
    };

    let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
    let result = match parser.parse(source, is_tsx) {
        Some(r) => r,
        None => return,
    };

    let root = result.tree.root_node();

    // Empty catch blocks.
    detect_empty_catches(rel_path, root, source, issues);

    // .then() without .catch() on same line.
    detect_unhandled_promises(rel_path, source, issues);

    // High complexity / deep nesting in code blocks.
    detect_high_complexity(rel_path, root, source, issues);
}

fn detect_empty_catches(rel_path: &str, root: tree_sitter::Node, source: &str, issues: &mut Vec<GotchaIssue>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "catch_clause" {
            let mut cursor = n.walk();
            for child in n.children(&mut cursor) {
                if child.kind() == "statement_block" || child.kind() == "block" {
                    let body_text = child.utf8_text(source.as_bytes()).unwrap_or("").trim();
                    let inner = body_text.trim_start_matches('{').trim_end_matches('}').trim();
                    if inner.is_empty() {
                        let line = n.start_position().row + 1;
                        issues.push(GotchaIssue {
                            file: rel_path.to_string(),
                            line,
                            rule: "empty-catch".into(),
                            severity: "warning".into(),
                            message: "Empty catch block silently swallows errors".into(),
                            confidence: 0.95,
                            snippet: truncate_line(source.lines().nth(line - 1).unwrap_or("")),
                        });
                    }
                }
            }
        }
        for i in (0..n.child_count()).rev() {
            if let Some(child) = n.child(i) {
                stack.push(child);
            }
        }
    }
}

fn detect_unhandled_promises(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>) {
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // Heuristic: line ends with .then(...) but doesn't contain .catch on the same line.
        if trimmed.contains(".then(") && !trimmed.contains(".catch(") && !is_comment_line(trimmed) {
            // Check if there's a .catch on the next few lines.
            let lines: Vec<&str> = source.lines().collect();
            let has_catch_nearby = (i + 1..std::cmp::min(i + 4, lines.len())).any(|j| lines[j].contains(".catch("));

            if !has_catch_nearby {
                issues.push(GotchaIssue {
                    file: rel_path.to_string(),
                    line: i + 1,
                    rule: "unhandled-promise".into(),
                    severity: "warning".into(),
                    message: "`.then()` without `.catch()` — unhandled promise rejection".into(),
                    confidence: 0.7,
                    snippet: truncate_line(line),
                });
            }
        }
    }
}

fn detect_high_complexity(rel_path: &str, root: tree_sitter::Node, source: &str, issues: &mut Vec<GotchaIssue>) {
    let is_test = is_test_file(rel_path);
    let blocks = crate::parse::blocks::extract_blocks(root, source.as_bytes());

    for block in &blocks {
        let metrics = compute_metrics_for_block(block);

        // High cyclomatic complexity — skip test/script files (less actionable).
        if metrics.complexity >= 20 {
            let severity = if is_test { "info" } else { "warning" };
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: block.start_line,
                rule: "high-complexity".into(),
                severity: severity.into(),
                message: format!(
                    "`{}` has complexity {} (threshold: 20) — consider breaking into smaller functions",
                    block.name, metrics.complexity
                ),
                confidence: 1.0,
                snippet: truncate_line(source.lines().nth(block.start_line - 1).unwrap_or("")),
            });
        }

        // Deep nesting — skip test/script files.
        if metrics.max_nesting_depth >= 5 {
            let severity = if is_test { "info" } else { "warning" };
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: block.start_line,
                rule: "deep-nesting".into(),
                severity: severity.into(),
                message: format!(
                    "`{}` has nesting depth {} (threshold: 5) — deeply nested code is error-prone",
                    block.name, metrics.max_nesting_depth
                ),
                confidence: 1.0,
                snippet: truncate_line(source.lines().nth(block.start_line - 1).unwrap_or("")),
            });
        }
    }
}

fn compute_metrics_for_block(block: &CodeBlock) -> crate::parse::complexity::ComplexityMetrics {
    // Re-parse the block's source to get its AST, then compute metrics.
    let parser = match AstParser::new() {
        Ok(p) => p,
        Err(_) => return crate::parse::complexity::ComplexityMetrics { complexity: 1, max_nesting_depth: 0 },
    };

    let result = match parser.parse(&block.source, false) {
        Some(r) => r,
        None => return crate::parse::complexity::ComplexityMetrics { complexity: 1, max_nesting_depth: 0 },
    };

    crate::parse::complexity::compute_metrics(result.tree.root_node(), block.source.as_bytes())
}

/// Detect callback hell (deeply nested callback functions).
pub fn detect_callback_hell(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>) {
    let mut depth: usize = 0;
    for (i, line) in source.lines().enumerate() {
        let before = depth;
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth = depth.saturating_sub(1);
            }
        }
        // Flag callback-like lines at nesting depth >= 4 (3+ levels deep).
        if before >= 4
            && !is_comment_line(line)
            && (line.contains("function(") || line.contains("function (") || line.contains("=>"))
        {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: i + 1,
                rule: "callback-hell".into(),
                severity: "info".into(),
                message: "Deeply nested callbacks, consider async/await".into(),
                confidence: 0.5,
                snippet: truncate_line(line),
            });
            break; // one issue per file is enough
        }
    }
}
