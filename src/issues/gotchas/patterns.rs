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
        if lang.is_js_family()
            && !is_test
            && !is_comment_line(line)
            && !is_example_or_script(rel_path)
            && (line.contains("console.log(") || line.contains("console.debug("))
        {
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
            && !is_comment_line(line)
        {
            let after = &line[idx..];
            if !after.contains("||") && !after.contains("??") && !after.contains("?.") && !after.contains("? ") {
                issues.push(GotchaIssue {
                    file: rel_path.to_string(),
                    line: line_num,
                    rule: "env-no-fallback".into(),
                    severity: "info".into(),
                    message: "`process.env.X` without fallback will be `undefined` if the env var is missing".into(),
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
                          `.ok()`, or proper error handling"
                    .into(),
                confidence: 0.5,
                snippet: truncate_line(line),
            });
        }

        // `panic!()` in non-test code.
        // Skip the deferred-panic idiom — `unwrap_or_else(|...| panic!(...))`
        // is the clippy-recommended way to attach a formatted message to
        // an unwrap (since `.expect(&format!(...))` triggers the
        // `expect_fun_call` lint). The error case still aborts, but the
        // rule's "use a Result" advice doesn't apply at this call site.
        if lang.is_rust()
            && !is_test
            && !is_comment_line(line)
            && calls_rust_macro(line, "panic")
            && !is_deferred_panic(line)
        {
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
        if lang.is_rust() && !is_comment_line(line) && calls_rust_macro(line, "todo") {
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
        if lang.is_rust() && !is_comment_line(line) && calls_rust_macro(line, "unimplemented") {
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
        if lang.is_rust()
            && !is_test
            && !is_example_or_script(rel_path)
            && !is_comment_line(line)
            && line.contains("println!(")
        {
            issues.push(GotchaIssue {
                file: rel_path.to_string(),
                line: line_num,
                rule: "rust-println".into(),
                severity: "info".into(),
                message: "`println!()` in library/application code; consider using a logging framework \
                          (tracing, log)"
                    .into(),
                confidence: 0.3,
                snippet: truncate_line(line),
            });
        }

        // `dbg!` in non-test code.
        if lang.is_rust() && !is_test && !is_comment_line(line) && calls_rust_macro(line, "dbg") {
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

/// Returns true if `line` looks like an actual call to the Rust macro
/// `name` (e.g. `todo`, `panic`, `unimplemented`) — i.e. `name!(`
/// appearing **outside** any string literal and **at a word boundary** so
/// `mytodo!(` doesn't match `todo`.
///
/// Skips occurrences inside both regular (`"…"`) and raw (`r"…"`,
/// `r#"…"#`, `r##"…"##`, …) string literals. This matters because the
/// gotcha rule definitions and unit tests in *this* file embed strings
/// like `"todo!("` — a naive `line.contains("todo!(")` would happily flag
/// the rule definition itself.
/// True if `line` looks like the deferred-panic idiom — `panic!(...)`
/// inside the closure passed to `unwrap_or_else` / `unwrap_or_else_with` /
/// `expect_or` / similar. Heuristic: the line has `unwrap_or_else(` *before*
/// the `panic!(` call. Doesn't try to span multiple lines because
/// rustfmt normally keeps the closure on one line.
fn is_deferred_panic(line: &str) -> bool {
    let Some(panic_at) = line.find("panic!(") else {
        return false;
    };
    let prefix = &line[..panic_at];
    prefix.contains("unwrap_or_else(") || prefix.contains("ok_or_else(")
}

pub(super) fn calls_rust_macro(line: &str, name: &str) -> bool {
    let needle = format!("{}!(", name);
    let needle_bytes = needle.as_bytes();
    let bytes = line.as_bytes();
    if bytes.len() < needle_bytes.len() {
        return false;
    }

    let mut i = 0usize;

    while i + needle_bytes.len() <= bytes.len() {
        let c = bytes[i];

        // Raw string: `r#…#"…"#…#`. Count the `#`s after the leading `r`
        // and skip past the matching `"<#×n>` terminator.
        if c == b'r' && i + 1 < bytes.len() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'#' {
                j += 1;
            }
            let hashes = j - (i + 1);
            if j < bytes.len() && bytes[j] == b'"' {
                // Find the closing `"<#×hashes>`.
                let mut k = j + 1;
                while k < bytes.len() {
                    if bytes[k] == b'"'
                        && bytes.len() >= k + 1 + hashes
                        && bytes[k + 1..k + 1 + hashes].iter().all(|&h| h == b'#')
                    {
                        i = k + 1 + hashes;
                        break;
                    }
                    k += 1;
                }
                if k >= bytes.len() {
                    // Unterminated raw string — bail out conservatively.
                    return false;
                }
                continue;
            }
        }

        // Regular `"…"` string with backslash escapes.
        if c == b'"' {
            let mut j = i + 1;
            while j < bytes.len() {
                let ch = bytes[j];
                if ch == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                    continue;
                }
                if ch == b'"' {
                    break;
                }
                j += 1;
            }
            if j >= bytes.len() {
                return false; // unterminated literal
            }
            i = j + 1;
            continue;
        }

        // Try the macro match here. Word-boundary on the left.
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let boundary_ok = i == 0 || {
                let prev = bytes[i - 1];
                !(prev.is_ascii_alphanumeric() || prev == b'_')
            };
            if boundary_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod calls_rust_macro_tests {
    use super::calls_rust_macro;

    #[test]
    fn matches_simple_call() {
        assert!(calls_rust_macro("    todo!(\"unfinished\");", "todo"));
        assert!(calls_rust_macro("panic!(\"boom\")", "panic"));
        assert!(calls_rust_macro("unimplemented!()", "unimplemented"));
    }

    #[test]
    fn ignores_match_inside_double_quoted_string() {
        // The exact false positive we're fixing: a rule definition that
        // mentions `todo!(` as a string literal must not flag itself.
        assert!(!calls_rust_macro(r#"line.contains("todo!(")"#, "todo"));
        assert!(!calls_rust_macro(r#"if line.contains("panic!(")"#, "panic"));
        assert!(!calls_rust_macro(r#"line.contains("unimplemented!(")"#, "unimplemented"));
    }

    #[test]
    fn requires_word_boundary() {
        // Word-boundary check: avoid matching `mytodo!(` when looking for `todo`.
        assert!(!calls_rust_macro("    mytodo!(x);", "todo"));
        assert!(!calls_rust_macro("    sub_todo!(x);", "todo"));
        // But a normal call after a non-word char does match.
        assert!(calls_rust_macro("foo();todo!();", "todo"));
        assert!(calls_rust_macro("(todo!())", "todo"));
    }

    #[test]
    fn no_match_when_absent() {
        assert!(!calls_rust_macro("let x = 1;", "todo"));
        assert!(!calls_rust_macro("// nothing here", "panic"));
    }

    #[test]
    fn handles_escaped_quote_inside_string() {
        // `\"` keeps us inside the string; the closing `"` after `todo!(`
        // is the real terminator.
        assert!(!calls_rust_macro(r#"println!("\"todo!(\" is a macro");"#, "todo"));
    }

    #[test]
    fn ignores_match_inside_raw_string() {
        // Raw strings — the actual case that bit us in this file's own tests.
        assert!(!calls_rust_macro(
            r####"assert!(!calls_rust_macro(r#"line.contains("todo!(")"#, "todo"));"####,
            "todo"
        ));
        assert!(!calls_rust_macro(r####"let s = r##"unimplemented!()"##;"####, "unimplemented"));
        // But a real call after a closing raw string still matches.
        assert!(calls_rust_macro(r#"let s = r"x"; todo!();"#, "todo"));
    }
}

#[cfg(test)]
mod is_deferred_panic_tests {
    use super::is_deferred_panic;

    #[test]
    fn matches_unwrap_or_else_panic() {
        // The exact idiom we're whitelisting.
        assert!(is_deferred_panic(
            r#"std::fs::create_dir_all(dir).unwrap_or_else(|_| panic!("create {} dir", dir.display()));"#
        ));
        assert!(is_deferred_panic(
            r#"        for e in std::fs::read_dir(&runtime_dir).unwrap_or_else(|_| panic!("read_dir")).flatten() {"#
        ));
    }

    #[test]
    fn matches_ok_or_else_panic() {
        assert!(is_deferred_panic(r#"let v = opt.ok_or_else(|| panic!("missing")).unwrap();"#));
    }

    #[test]
    fn does_not_match_top_level_panic() {
        // The real anti-pattern the rule should still catch.
        assert!(!is_deferred_panic(r#"    panic!("unrecoverable: {}", err);"#));
        assert!(!is_deferred_panic(r#"if bad { panic!("nope"); }"#));
    }

    #[test]
    fn does_not_match_panic_with_unrelated_method_chain() {
        // `.unwrap_or_else(...)` *after* a separate `panic!(...)` should still flag.
        assert!(!is_deferred_panic(r#"panic!("first"); something.unwrap_or_else(|_| 0);"#));
    }

    #[test]
    fn returns_false_when_no_panic() {
        assert!(!is_deferred_panic("let x = foo.unwrap_or_else(|_| 0);"));
    }
}
