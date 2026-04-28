use tree_sitter::{Node, Parser, Tree};

/// Wrapper around tree-sitter for TypeScript/TSX parsing.
pub struct AstParser {
    parser: Parser,
}

impl AstParser {
    pub fn new() -> Result<Self, tree_sitter::LanguageError> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang)?;
        Ok(Self { parser })
    }

    /// Parse source code. If `is_tsx` is true, use the TSX grammar.
    /// Returns the parse tree and whether any ERROR nodes were found.
    pub fn parse(&mut self, code: &str, is_tsx: bool) -> Option<ParseResult> {
        let lang: tree_sitter::Language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        let _ = self.parser.set_language(&lang);
        let tree = self.parser.parse(code, None)?;
        let has_errors = tree.root_node().has_error();
        Some(ParseResult {
            tree,
            has_errors,
        })
    }
}

pub struct ParseResult {
    pub tree: Tree,
    pub has_errors: bool,
}

// ---------------------------------------------------------------------------
// AST extraction helpers
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

/// Collect all ERROR nodes in the tree.
/// `source` is the original source bytes (needed for error text).
pub fn collect_errors(root: Node, source: &[u8]) -> Vec<(String, usize, usize)> {
    let mut errors = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.is_error() || n.is_missing() {
            let msg = if n.is_missing() {
                format!("missing {}", n.kind())
            } else {
                let text = n.utf8_text(source).unwrap_or("?");
                format!("unexpected token: {}", &text[..text.len().min(80)])
            };
            let start = n.start_position();
            errors.push((msg, start.row + 1, start.column + 1));
            // Don't recurse into error nodes.
        } else {
            for i in (0..n.child_count()).rev() {
                if let Some(child) = n.child(i) {
                    stack.push(child);
                }
            }
        }
    }
    errors
}

/// Extract import specifiers from an AST root node.
/// Returns (internal_imports, external_imports).
/// Internal imports start with "." or ".." or "/".
pub fn extract_imports(root: Node, source: &str) -> (Vec<String>, Vec<String>) {
    let mut internal = Vec::new();
    let mut external = Vec::new();

    // Static import statements: import ... from 'module'
    let import_nodes = collect_nodes(root, &["import_statement"]);
    for node in import_nodes {
        if let Some(spec) = extract_module_specifier(node, source) {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // Export-from statements: export ... from 'module'
    let export_nodes = collect_nodes(root, &["export_statement"]);
    for node in export_nodes {
        if let Some(spec) = extract_module_specifier(node, source) {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // Dynamic import() expressions.
    let dynamic_imports = collect_nodes(root, &["import_expression"]);
    for node in dynamic_imports {
        if let Some(spec) = extract_module_specifier(node, source) {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // require() calls.
    let call_exprs = collect_nodes(root, &["call_expression"]);
    for call in &call_exprs {
        if let Some(func) = call.child(0)
            && func.kind() == "identifier"
                && func.utf8_text(source.as_bytes()).unwrap_or("") == "require"
            && let Some(spec) = extract_module_specifier(*call, source)
        {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    internal.sort();
    external.sort();
    (internal, external)
}

/// Extract the module specifier (the "from '...'" string) from a node.
/// This looks for a `string` child that appears after a `from` keyword,
/// or the last `string` child in import/require-like nodes.
fn extract_module_specifier(node: Node, source: &str) -> Option<String> {
    // For import_statement and export_statement, find the string after "from".
    let mut found_from = false;
    let mut last_string: Option<String> = None;
    for i in 0..node.child_count() {
        let child = node.child(i)?;
        if child.kind() == "from" {
            found_from = true;
            continue;
        }
        if child.kind() == "string" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            let spec = unquote(text);
            if found_from {
                return Some(spec);
            }
            last_string = Some(spec);
        }
    }

    // For import_statement without "from" (side-effect import): import 'module'
    // For import_expression and call_expression: the argument string.
    if last_string.is_some() {
        return last_string;
    }

    None
}

fn classify_import(spec: &str, internal: &mut Vec<String>, external: &mut Vec<String>) {
    if spec.starts_with('.') || spec.starts_with('/') {
        if !internal.contains(&spec.to_string()) {
            internal.push(spec.to_string());
        }
    } else if !spec.is_empty() {
        let pkg = extract_package_name(spec);
        if !external.contains(&pkg) {
            external.push(pkg);
        }
    }
}

/// Count functions in the AST.
pub fn count_functions(root: Node) -> usize {
    let kinds = [
        "function_declaration",
        "function_expression",
        "arrow_function",
        "method_definition",
        "generator_function_declaration",
        "generator_function",
    ];
    collect_nodes(root, &kinds).len()
}

/// Count classes in the AST.
pub fn count_classes(root: Node) -> usize {
    let kinds = [
        "class_declaration",
        "class_expression",
    ];
    collect_nodes(root, &kinds).len()
}

/// Compute cyclomatic complexity approximation.
pub fn compute_complexity(root: Node, source: &str) -> usize {
    let decision_kinds = [
        "if_statement",
        "for_statement",
        "for_in_statement",
        "while_statement",
        "do_statement",
        "switch_case",
        "catch_clause",
        "ternary_expression",
    ];

    let mut complexity = 1; // base
    let nodes = collect_nodes(root, &decision_kinds);
    complexity += nodes.len();

    // Count logical operators (&&, ||, ??) in binary expressions.
    let binary_exprs = collect_nodes(root, &["binary_expression"]);
    for expr in &binary_exprs {
        if let Some(child) = expr.child_by_field_name("operator") {
            let op = child.utf8_text(source.as_bytes()).unwrap_or("");
            if op == "&&" || op == "||" || op == "??" {
                complexity += 1;
            }
        }
    }

    complexity
}

/// Count non-empty, non-comment lines of code.
pub fn count_loc(source: &str) -> (usize, usize) {
    let total = source.lines().count();
    let mut loc = 0;
    let mut in_block_comment = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }
        loc += 1;
    }
    (loc, total)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Remove surrounding quotes from a string literal.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Extract the package name from an import specifier.
/// e.g. "lodash/merge" → "lodash", "@angular/core" → "@angular/core"
fn extract_package_name(spec: &str) -> String {
    if spec.starts_with('@') {
        let parts: Vec<&str> = spec.splitn(3, '/').collect();
        if parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            spec.to_string()
        }
    } else {
        spec.split('/').next().unwrap_or(spec).to_string()
    }
}

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
    fn test_extract_package_name() {
        assert_eq!(extract_package_name("lodash"), "lodash");
        assert_eq!(extract_package_name("lodash/merge"), "lodash");
        assert_eq!(extract_package_name("@angular/core"), "@angular/core");
        assert_eq!(
            extract_package_name("@angular/core/testing"),
            "@angular/core"
        );
    }

    #[test]
    fn test_count_loc() {
        let code = r#"
// comment
const x = 1;

/* block
   comment */
const y = 2;
"#;
        let (loc, total) = count_loc(code);
        assert_eq!(loc, 2);
        assert!(total > loc);
    }

    #[test]
    fn test_parse_simple_typescript() {
        let mut parser = AstParser::new().expect("parser init");
        let code = "const x: number = 1;";
        let result = parser.parse(code, false).expect("parse");
        assert!(!result.has_errors);
    }

    #[test]
    fn test_parse_malformed_typescript() {
        let mut parser = AstParser::new().expect("parser init");
        let code = "function ( { } } }";
        let result = parser.parse(code, false).expect("parse");
        assert!(result.has_errors);
    }

    #[test]
    fn test_extract_imports() {
        let mut parser = AstParser::new().expect("parser init");
        let code = r#"
import { foo } from './utils';
import bar from '../lib/bar';
import * as _ from 'lodash';
"#;
        let result = parser.parse(code, false).expect("parse");
        let (internal, external) = extract_imports(result.tree.root_node(), code);
        assert!(internal.contains(&"../lib/bar".to_string()));
        assert!(internal.contains(&"./utils".to_string()));
        assert!(external.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_count_functions() {
        let mut parser = AstParser::new().expect("parser init");
        let code = r#"
function foo() {}
const bar = () => {};
class Baz {
    method() {}
}
"#;
        let result = parser.parse(code, false).expect("parse");
        assert_eq!(count_functions(result.tree.root_node()), 3);
    }

    #[test]
    fn test_count_classes() {
        let mut parser = AstParser::new().expect("parser init");
        let code = "class Foo {} class Bar {}";
        let result = parser.parse(code, false).expect("parse");
        assert_eq!(count_classes(result.tree.root_node()), 2);
    }
}
