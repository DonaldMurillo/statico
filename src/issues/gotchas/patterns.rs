//! Pattern-based gotcha detection (regex/text matching).
//!
//! Detects common anti-patterns like `any` types, loose equality, eval, etc.

use crate::types::GotchaIssue;

use super::{is_comment_line, is_example_or_script, is_test_file, truncate_line};

/// Detect gotchas in a single file using pattern matching.
pub fn detect_in_file(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>) {
    let is_test = is_test_file(rel_path);
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        // == instead of === (TypeScript/ESLint catches this; only flag at low confidence)
        if !is_test && !is_comment_line(line) && !is_example_or_script(rel_path) {
            // Only flag bare == not inside >=, <=, =>, !=, !==, ===
            let has_bare_eq = line.contains("==")
                && !line.contains("===")
                && !line.contains("!==")
                && !line.contains("<=")
                && !line.contains(">=")
                && !line.contains("=>");
            if has_bare_eq {
                issues.push(GotchaIssue {
                    file: rel_path.to_string(),
                    line: line_num,
                    rule: "loose-equality".into(),
                    severity: "info".into(),
                    message: "Use `===` instead of `==` to avoid type coercion bugs".into(),
                    confidence: 0.4,
                    snippet: truncate_line(line),
                });
            }
        }

        // != instead of !== (same reasoning)
        if !is_test && !is_comment_line(line) && !is_example_or_script(rel_path) {
            let has_bare_neq = line.contains("!=") && !line.contains("!==");
            if has_bare_neq {
                issues.push(GotchaIssue {
                    file: rel_path.to_string(),
                    line: line_num,
                    rule: "loose-inequality".into(),
                    severity: "info".into(),
                    message: "Use `!==` instead of `!=` to avoid type coercion bugs".into(),
                    confidence: 0.4,
                    snippet: truncate_line(line),
                });
            }
        }

        // eval() usage — skip in test files (often testing eval attack vectors).
        if line.contains("eval(") && !is_comment_line(line) && !is_test {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "eval-usage".into(),
                severity: "critical".into(),
                message: "`eval()` is a code injection risk".into(),
                confidence: 0.95,
                snippet: truncate_line(line),
            });
        }

        // innerHTML assignment — skip in test files.
        if line.contains(".innerHTML") && !is_comment_line(line) && !is_test {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "xss-innerhtml".into(),
                severity: "critical".into(),
                message: "`.innerHTML` assignment is an XSS risk; use textContent or a sanitization library".into(),
                confidence: 0.9,
                snippet: truncate_line(line),
            });
        }

        // dangerouslySetInnerHTML — skip in test files.
        if line.contains("dangerouslySetInnerHTML") && !is_comment_line(line) && !is_test {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "xss-dangerously-set".into(),
                severity: "critical".into(),
                message: "`dangerouslySetInnerHTML` bypasses React's XSS protection".into(),
                confidence: 0.95,
                snippet: truncate_line(line),
            });
        }

        // `: any` type annotation.
        if line.contains(": any") && !is_comment_line(line) {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "any-type".into(),
                severity: "warning".into(),
                message: "`any` type bypasses type checking".into(),
                confidence: 0.9,
                snippet: truncate_line(line),
            });
        }

        // `as any` cast.
        if line.contains("as any") && !is_comment_line(line) {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "as-any-cast".into(),
                severity: "warning".into(),
                message: "`as any` cast defeats type safety".into(),
                confidence: 0.95,
                snippet: truncate_line(line),
            });
        }

        // `<any>` cast.
        if line.contains("<any>") && !is_comment_line(line) {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "any-cast-angle".into(),
                severity: "warning".into(),
                message: "`<any>` cast defeats type safety".into(),
                confidence: 0.7,
                snippet: truncate_line(line),
            });
        }

        // Console statements: only flag console.log/debug (not warn/error which are
        // intentional error handling). Skip in test, example, script, and server files.
        if !is_test && !is_comment_line(line) && !is_example_or_script(rel_path)
            && (line.contains("console.log(") || line.contains("console.debug(")) {
                issues.push(GotchaIssue {
                    file: rel_path.to_string(),
                    line: line_num,
                    rule: "console-statement".into(),
                    severity: "info".into(),
                    message: "Console statement left in production code".into(),
                    confidence: 0.4,
                    snippet: truncate_line(line),
                });
            }

        // Unresolved TODO/FIXME/HACK/XXX comments.
        if is_comment_line(line) {
            for tag in &["TODO:", "FIXME:", "HACK:", "XXX:"] {
                if line.contains(tag) {
                    issues.push(GotchaIssue {
                        file: rel_path.to_string(),
                        line: line_num,
                        rule: "unresolved-comment".into(),
                        severity: "info".into(),
                        message: format!("Unresolved {} comment", tag.trim_end_matches(':')),
                        confidence: 0.5,
                        snippet: truncate_line(line),
                    });
                    break; // one issue per line
                }
            }
        }

        // process.env.VAR without fallback.
        if let Some(idx) = line.find("process.env.")
            && !is_comment_line(line) {
                // Check if there's a fallback after the env var access.
                let after = &line[idx..];
                // Simple heuristic: if no `||` or `??` or `?.` or ternary nearby, flag it.
                if !after.contains("||") && !after.contains("??") && !after.contains("?.") && !after.contains("? ") {
                    issues.push(GotchaIssue {
                        file: rel_path.to_string(),
                        line: line_num,
                        rule: "env-no-fallback".into(),
                        severity: "info".into(),
                        message: "`process.env.X` without fallback will be `undefined` if the env var is missing"
                            .into(),
                        confidence: 0.7,
                        snippet: truncate_line(line),
                    });
                }
            }
    }
}
