//! Cyclomatic complexity and nesting depth measurement.

use tree_sitter::Node;

/// Complexity + nesting metrics for a code unit (function, file, etc).
#[derive(Debug, Clone)]
pub struct ComplexityMetrics {
    /// Cyclomatic complexity (decision points + 1).
    pub complexity: usize,
    /// Maximum nesting depth of control-flow constructs.
    pub max_nesting_depth: usize,
}

/// Compute cyclomatic complexity for an AST subtree.
///
/// Counts decision points: `if`, `else`, `for`, `while`, `do`, `catch`,
/// `&&`, `||`, `??`, ternary `?`, and `switch` cases.
pub fn compute_complexity(root: Node, source: &[u8]) -> usize {
    compute_metrics(root, source).complexity
}

/// Compute both cyclomatic complexity and max nesting depth.
pub fn compute_metrics(root: Node, source: &[u8]) -> ComplexityMetrics {
    let mut complexity: usize = 1; // Base complexity
    let mut max_depth: usize = 0;

    // Stack entries: (node, nesting_depth)
    let mut stack: Vec<(Node, usize)> = vec![(root, 0)];

    while let Some((n, depth)) = stack.pop() {
        let mut new_depth = depth;

        match n.kind() {
            "if_statement" => {
                complexity += 1;
                new_depth = depth + 1;
            }
            "else_clause" => {
                complexity += 1;
                // else is at same depth as its if.
                new_depth = depth;
            }
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
                complexity += 1;
                new_depth = depth + 1;
            }
            "catch_clause" => {
                complexity += 1;
                new_depth = depth + 1;
            }
            "ternary_expression" => {
                complexity += 1;
                // Don't increment nesting for ternary — it's inline.
            }
            "switch_case" | "switch_default" => {
                complexity += 1;
                new_depth = depth + 1;
            }
            "&&" | "||" | "??"
                if n.child_count() == 0
                    && n.utf8_text(source).map(|t| matches!(t, "&&" | "||" | "??")).unwrap_or(false) =>
            {
                complexity += 1;
            }
            _ => {}
        }

        if new_depth > max_depth {
            max_depth = new_depth;
        }

        for i in (0..n.child_count()).rev() {
            if let Some(child) = n.child(i) {
                stack.push((child, new_depth));
            }
        }
    }

    ComplexityMetrics { complexity, max_nesting_depth: max_depth }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::AstParser;

    #[test]
    fn test_simple_function() {
        let p = AstParser::new().unwrap();
        let code = "function foo() { return 1; }";
        let r = p.parse(code, false).unwrap();
        assert_eq!(compute_complexity(r.tree.root_node(), code.as_bytes()), 1);
    }

    #[test]
    fn test_if_else() {
        let p = AstParser::new().unwrap();
        let code = "function foo(x) { if (x > 0) { return 1; } else { return -1; } }";
        let r = p.parse(code, false).unwrap();
        assert_eq!(compute_complexity(r.tree.root_node(), code.as_bytes()), 3);
    }

    #[test]
    fn test_switch() {
        let p = AstParser::new().unwrap();
        let code = "function foo(x) { switch(x) { case 1: break; case 2: break; default: break; } }";
        let r = p.parse(code, false).unwrap();
        // 1 base + 2 cases + 1 default = 4
        assert_eq!(compute_complexity(r.tree.root_node(), code.as_bytes()), 4);
    }

    #[test]
    fn test_logical_operators() {
        let p = AstParser::new().unwrap();
        let code = "function foo(a, b, c) { return a && b || c; }";
        let r = p.parse(code, false).unwrap();
        // 1 base + && + ||
        assert_eq!(compute_complexity(r.tree.root_node(), code.as_bytes()), 3);
    }

    #[test]
    fn test_loops_and_catch() {
        let p = AstParser::new().unwrap();
        let code = r#"
function foo(items) {
    for (const item of items) { console.log(item); }
    try { risky(); } catch (e) { handle(e); }
}
"#;
        let r = p.parse(code, false).unwrap();
        // 1 base + for + catch = 3
        assert_eq!(compute_complexity(r.tree.root_node(), code.as_bytes()), 3);
    }

    #[test]
    fn test_nesting_depth_flat() {
        let p = AstParser::new().unwrap();
        let code = "function foo() { return 1; }";
        let r = p.parse(code, false).unwrap();
        let m = compute_metrics(r.tree.root_node(), code.as_bytes());
        assert_eq!(m.max_nesting_depth, 0);
    }

    #[test]
    fn test_nesting_depth_nested_loops() {
        let p = AstParser::new().unwrap();
        let code = "function foo(items) { for (const a of items) { for (const b of a) { if (b) {} } } }";
        let r = p.parse(code, false).unwrap();
        let m = compute_metrics(r.tree.root_node(), code.as_bytes());
        // for(1) → for(2) → if(3)
        assert_eq!(m.max_nesting_depth, 3);
        // 1 base + 2 for + 1 if = 4
        assert_eq!(m.complexity, 4);
    }

    #[test]
    fn test_nesting_depth_deep_if_chain() {
        let p = AstParser::new().unwrap();
        let code = "function foo(a,b,c,d) { if(a){if(b){if(c){if(d){}}}}}";
        let r = p.parse(code, false).unwrap();
        let m = compute_metrics(r.tree.root_node(), code.as_bytes());
        // 4 nested ifs
        assert_eq!(m.max_nesting_depth, 4);
    }
}
