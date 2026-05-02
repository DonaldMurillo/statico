//! Parse error collection from tree-sitter AST.

use tree_sitter::Node;

/// Collect all ERROR nodes from the AST.
/// Returns (message, line, column) tuples.
pub fn collect_errors(root: Node, source: &[u8]) -> Vec<(String, usize, usize)> {
    let mut errors = Vec::new();
    let mut stack = vec![root];

    while let Some(n) = stack.pop() {
        if n.is_error() || n.is_missing() {
            let msg = if n.is_missing() {
                format!("missing {}", n.kind())
            } else {
                let text = n.utf8_text(source).unwrap_or("?");
                let preview: String = text.chars().take(80).collect();
                format!("unexpected token: {}", preview)
            };
            errors.push((msg, n.start_position().row + 1, n.start_position().column + 1));
        }
        for i in (0..n.child_count()).rev() {
            if let Some(child) = n.child(i) {
                stack.push(child);
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::AstParser;

    #[test]
    fn test_no_errors() {
        let p = AstParser::new().unwrap();
        let code = "const x: number = 1;";
        let r = p.parse(code, false).unwrap();
        let errs = collect_errors(r.tree.root_node(), code.as_bytes());
        assert!(errs.is_empty());
    }

    #[test]
    fn test_with_errors() {
        let p = AstParser::new().unwrap();
        let code = "function ( { } } }";
        let r = p.parse(code, false).unwrap();
        let errs = collect_errors(r.tree.root_node(), code.as_bytes());
        assert!(!errs.is_empty());
        // Verify line/column are populated.
        assert!(errs[0].1 > 0);
    }
}
