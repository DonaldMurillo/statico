//! File-level metrics: lines of code, function count, class count.

use tree_sitter::Node;

use super::collect_nodes;

/// Count lines of code (non-blank, non-comment) and total lines.
pub fn count_loc(source: &str) -> (usize, usize) {
    let total = source.lines().count();
    let loc = source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .count();
    (loc, total)
}

/// Count function declarations, expressions, and arrow functions.
pub fn count_functions(root: Node) -> usize {
    let kinds = ["function_declaration", "generator_function_declaration", "function_expression", "arrow_function"];
    collect_nodes(root, &kinds).len()
}

/// Count class declarations and expressions.
pub fn count_classes(root: Node) -> usize {
    let kinds = ["class_declaration", "class_expression"];
    collect_nodes(root, &kinds).len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::AstParser;

    #[test]
    fn count_loc_simple() {
        let code = "const x = 1;\n\n// comment\nconst y = 2;\n";
        let (loc, total) = count_loc(code);
        assert_eq!(loc, 2);
        assert_eq!(total, 4); // .lines() trims trailing newline
    }

    #[test]
    fn count_loc_blank_only() {
        let code = "\n\n\n";
        let (loc, total) = count_loc(code);
        assert_eq!(loc, 0);
        assert_eq!(total, 3);
    }

    #[test]
    fn count_functions_simple() {
        let p = AstParser::new().unwrap();
        let code = "function foo() {} const bar = () => {};";
        let r = p.parse(code, false).unwrap();
        assert_eq!(count_functions(r.tree.root_node()), 2);
    }

    #[test]
    fn count_classes_simple() {
        let p = AstParser::new().unwrap();
        let code = "class Foo {} class Bar {}";
        let r = p.parse(code, false).unwrap();
        assert_eq!(count_classes(r.tree.root_node()), 2);
    }

    #[test]
    fn count_functions_empty() {
        let p = AstParser::new().unwrap();
        let code = "const x = 1;";
        let r = p.parse(code, false).unwrap();
        assert_eq!(count_functions(r.tree.root_node()), 0);
    }
}
