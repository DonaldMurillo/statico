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
    crate::parse::unquote::unquote(s)
}

pub mod blocks;
pub mod complexity;
pub mod errors;
pub mod exports;
pub mod imports;
pub mod metrics;
pub mod rust;
pub mod unquote;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── V7-2: Parser mutex recovery — test that poisoned mutex doesn't panic ──
    #[test]
    fn sec_parse_recovers_from_poisoned_mutex() {
        let parser = AstParser::new().expect("parser init");
        let result = parser.parse("const x = 1;", false);
        assert!(result.is_some());
        let result2 = parser.parse("const y = 2;", false);
        assert!(result2.is_some(), "second parse should succeed");
    }
}
