//! Pattern-based gotcha detection (regex/text matching).
//!
//! Detects common anti-patterns like `any` types, loose equality, eval, etc.
//! Rules are filtered by language — TypeScript-only rules won't fire on Rust files.

use crate::types::GotchaIssue;

use super::{is_comment_line, is_example_or_script, is_test_file, truncate_line};

/// Which language family the file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileLanguage {
    TypeScript,
    JavaScript,
    Rust,
    Unknown,
}

impl FileLanguage {
    pub fn from_path(path: &str) -> Self {
        match path.rsplit('.').next().unwrap_or("") {
            "ts" | "tsx" => FileLanguage::TypeScript,
            "js" | "jsx" => FileLanguage::JavaScript,
            "rs" => FileLanguage::Rust,
            _ => FileLanguage::Unknown,
        }
    }

    /// True for TypeScript or JavaScript files.
    pub fn is_js_family(self) -> bool {
        matches!(self, FileLanguage::TypeScript | FileLanguage::JavaScript)
    }

    /// True for Rust files.
    pub fn is_rust(self) -> bool {
        matches!(self, FileLanguage::Rust)
    }
}

/// Detect gotchas in a single file using pattern matching.
pub fn detect_in_file(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>) {
    let lang = FileLanguage::from_path(rel_path);
    let is_test = is_test_file(rel_path);
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        // == instead of === (TypeScript/ESLint catches this; only flag at low confidence)
        // JS/TS-only rule — Rust uses == legitimately.
        if lang.is_js_family() && !is_test && !is_comment_line(line) && !is_example_or_script(rel_path) {
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

        // != instead of !== (JS/TS-only)
        if lang.is_js_family() && !is_test && !is_comment_line(line) && !is_example_or_script(rel_path) {
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
        // JS/TS-only — Rust doesn't have eval().
        if lang.is_js_family() && line.contains("eval(") && !is_comment_line(line) && !is_test {
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

        // innerHTML assignment — JS/TS-only.
        if lang.is_js_family() && line.contains(".innerHTML") && !is_comment_line(line) && !is_test {
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

        // dangerouslySetInnerHTML — JS/TS-only.
        if lang.is_js_family() && line.contains("dangerouslySetInnerHTML") && !is_comment_line(line) && !is_test {
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

        // `: any` type annotation — TypeScript-only.
        if lang.is_js_family() && line.contains(": any") && !is_comment_line(line) {
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

        // `as any` cast — TypeScript-only.
        if lang.is_js_family() && line.contains("as any") && !is_comment_line(line) {
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

        // `<any>` cast — TypeScript-only.
        if lang.is_js_family() && line.contains("<any>") && !is_comment_line(line) {
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

        // Console statements — JS/TS-only.
        if lang.is_js_family() && !is_test && !is_comment_line(line) && !is_example_or_script(rel_path)
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

        // Unresolved TODO/FIXME/HACK/XXX comments — language-agnostic.
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

        // process.env.VAR without fallback — JS/TS-only.
        if lang.is_js_family()
            && let Some(idx) = line.find("process.env.")
                && !is_comment_line(line) {
                    let after = &line[idx..];
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

        // ── Rust-specific rules ────────────────────────────────────────
        // Rust uses `//` comments too, so unresolved-comment detection works.

        // `unwrap()` in non-test code — panics at runtime.
        if lang.is_rust() && !is_test && !is_comment_line(line) && line.contains(".unwrap()") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-unwrap".into(),
                severity: "warning".into(),
                message: "`.unwrap()` can panic at runtime; consider `.unwrap_or_default()`, \
                          `.ok()`, or proper error handling".into(),
                confidence: 0.5,
                snippet: truncate_line(line),
            });
        }

        // `panic!()` in non-test code.
        if lang.is_rust() && !is_test && !is_comment_line(line) && line.contains("panic!(") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-panic".into(),
                severity: "warning".into(),
                message: "`panic!()` aborts the thread/process; use `Result` for recoverable errors".into(),
                confidence: 0.8,
                snippet: truncate_line(line),
            });
        }

        // `todo!()` macro — unimplemented code.
        if lang.is_rust() && !is_comment_line(line) && line.contains("todo!(") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-todo".into(),
                severity: "warning".into(),
                message: "`todo!()` will panic at runtime when reached".into(),
                confidence: 0.9,
                snippet: truncate_line(line),
            });
        }

        // `unimplemented!()` macro.
        if lang.is_rust() && !is_comment_line(line) && line.contains("unimplemented!(") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-unimplemented".into(),
                severity: "warning".into(),
                message: "`unimplemented!()` will panic at runtime when reached".into(),
                confidence: 0.9,
                snippet: truncate_line(line),
            });
        }

        // `println!` in non-test, non-example code (should use logging).
        if lang.is_rust() && !is_test && !is_example_or_script(rel_path) && !is_comment_line(line)
            && line.contains("println!(") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-println".into(),
                severity: "info".into(),
                message: "`println!()` in library/application code; consider using a logging framework \
                          (tracing, log)".into(),
                confidence: 0.3,
                snippet: truncate_line(line),
            });
        }

        // `dbg!` in non-test code.
        if lang.is_rust() && !is_test && !is_comment_line(line) && line.contains("dbg!(") {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-dbg".into(),
                severity: "info".into(),
                message: "`dbg!()` is a debugging macro; remove before committing".into(),
                confidence: 0.7,
                snippet: truncate_line(line),
            });
        }
    }
}
