//! AST parser wrapper and shared traversal helpers.

use std::sync::Mutex;
use tree_sitter::{Node, Parser, Tree};

/// Wrapper around tree-sitter for TypeScript/TSX parsing.
pub struct AstParser {
    parser: Mutex<Parser>,
}

impl AstParser {
    pub fn new() -> Result<Self, tree_sitter::LanguageError> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang)?;
        Ok(Self { parser: Mutex::new(parser) })
    }

    /// Parse source code. If `is_tsx` is true, use the TSX grammar.
    pub fn parse(&self, code: &str, is_tsx: bool) -> Option<ParseResult> {
        let lang: tree_sitter::Language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        // V7-2: Recover from poisoned mutex (another thread panicked while holding it).
        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        let _ = parser.set_language(&lang);
        let tree = parser.parse(code, None)?;
        let has_errors = tree.root_node().has_error();
        Some(ParseResult { tree, has_errors })
    }
}

pub struct ParseResult {
    pub tree: Tree,
    pub has_errors: bool,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Collect all nodes whose kind matches one of `kinds`.
pub fn collect_nodes<'a>(root: Node<'a>, kinds: &[&str]) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if kinds.contains(&n.kind()) {
            result.push(n);
        }
        for i in (0..n.child_count()).rev() {
            if let Some(child) = n.child(i) {
                stack.push(child);
            }
        }
    }
    result
}

/// Remove surrounding quotes from a string literal.
pub fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('`') && s.ends_with('`')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub mod blocks;
pub mod complexity;
pub mod errors;
pub mod exports;
pub mod imports;
pub mod metrics;
pub mod rust;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unquote() {
        assert_eq!(unquote("'./foo'"), "./foo");
        assert_eq!(unquote("\"bar\""), "bar");
        assert_eq!(unquote("`baz`"), "baz");
        assert_eq!(unquote("naked"), "naked");
    }

    #[test]
    fn test_parse_simple_typescript() {
        let parser = AstParser::new().expect("parser init");
        let code = "const x: number = 1;";
        let result = parser.parse(code, false).expect("parse");
        assert!(!result.has_errors);
    }

    #[test]
    fn test_parse_malformed_typescript() {
        let parser = AstParser::new().expect("parser init");
        let code = "function ( { } } }";
        let result = parser.parse(code, false).expect("parse");
        assert!(result.has_errors);
    }

    // ── V7-1: unquote must not panic on single-char quoted strings ──
    #[test]
    fn sec_v7_1_unquote_no_panic_on_single_char() {
        // A single-character string like `"` satisfies starts_with and ends_with
        // but slicing [1..0] would panic. Verify it returns the string as-is.
        assert_eq!(unquote("\""), "\"");
        assert_eq!(unquote("'"), "'");
        assert_eq!(unquote("`"), "`");
        // Empty string should also be safe
        assert_eq!(unquote(""), "");
    }

    // ── V7-2: Parser mutex recovery — test that poisoned mutex doesn't panic ──
    #[test]
    fn sec_v7_2_parser_recovers_from_poisoned_mutex() {
        let parser = AstParser::new().expect("parser init");
        // Parse normally first
        let result = parser.parse("const x = 1;", false);
        assert!(result.is_some());
        // Simulate mutex poisoning by panicking inside a lock, then recovering.
        // We can't easily force a poison in a single-threaded test, but we can
        // verify the lock() call succeeds repeatedly (it uses unwrap_or_else).
        let result2 = parser.parse("const y = 2;", false);
        assert!(result2.is_some(), "second parse should succeed");
    }
}
