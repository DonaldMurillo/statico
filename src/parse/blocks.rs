//! Code block extraction — pull named functions, methods, and arrow-function const
//! declarations from an AST so we can fingerprint them for duplicate detection.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

/// A named code block with its source text and location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub name: String,
    pub source: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockKind {
    Function,
    Method,
    ArrowFunction,
    Class,
    Fragment,
}

/// Extract all named code blocks from an AST.
pub fn extract_blocks(root: Node, source: &[u8]) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    collect_blocks_recursive(root, source, &mut blocks);
    blocks
}

fn collect_blocks_recursive(node: Node, source: &[u8], blocks: &mut Vec<CodeBlock>) {
    match node.kind() {
        "function_declaration" | "function" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                push_block(node, name_node, source, BlockKind::Function, blocks);
            }
        }
        "generator_function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                push_block(node, name_node, source, BlockKind::Function, blocks);
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            // Handle: const myFunc = () => { ... } or function expression
            extract_declarator_blocks(node, source, blocks);
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                push_block(node, name_node, source, BlockKind::Class, blocks);
            }
        }
        "method_definition" => {
            // Class methods.
            if let Some(name_node) = node.child_by_field_name("name") {
                push_block(node, name_node, source, BlockKind::Method, blocks);
            }
        }
        "public_field_definition" => {
            // Class arrow-function properties: myMethod = () => { ... }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if (child.kind() == "arrow_function" || child.kind() == "function_expression")
                    && let Some(name_node) = node.child_by_field_name("name") {
                        let name = name_node.utf8_text(source).unwrap_or("anonymous").to_string();
                        let start = child.start_position().row + 1;
                        let end = child.end_position().row + 1;
                        let body_text = child.utf8_text(source).unwrap_or("").to_string();
                        // Skip trivially short blocks.
                        if body_text.lines().count() >= 3 {
                            blocks.push(CodeBlock {
                                name,
                                source: body_text,
                                start_line: start,
                                end_line: end,
                                kind: BlockKind::ArrowFunction,
                            });
                        }
                    }
            }
        }
        _ => {}
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_blocks_recursive(child, source, blocks);
    }
}

fn extract_declarator_blocks(node: Node, source: &[u8], blocks: &mut Vec<CodeBlock>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name");
            let value_node = child.child_by_field_name("value");
            if let (Some(name_node), Some(value_node)) = (name_node, value_node) {
                let is_arrow = value_node.kind() == "arrow_function";
                let is_func_expr = value_node.kind() == "function_expression";
                if is_arrow || is_func_expr {
                    let name = name_node.utf8_text(source).unwrap_or("anonymous").to_string();
                    let start = value_node.start_position().row + 1;
                    let end = value_node.end_position().row + 1;
                    let body_text = value_node.utf8_text(source).unwrap_or("").to_string();
                    if body_text.lines().count() >= 3 {
                        blocks.push(CodeBlock {
                            name,
                            source: body_text,
                            start_line: start,
                            end_line: end,
                            kind: BlockKind::ArrowFunction,
                        });
                    }
                }
            }
        }
    }
}

fn push_block(node: Node, name_node: Node, source: &[u8], kind: BlockKind, blocks: &mut Vec<CodeBlock>) {
    let name = name_node.utf8_text(source).unwrap_or("anonymous").to_string();
    let start = node.start_position().row + 1;
    let end = node.end_position().row + 1;
    let body_text = node.utf8_text(source).unwrap_or("").to_string();
    // Only include blocks with enough substance (3+ lines).
    if body_text.lines().count() >= 3 {
        blocks.push(CodeBlock { name, source: body_text, start_line: start, end_line: end, kind });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::AstParser;

    fn parse_blocks(source: &str) -> Vec<CodeBlock> {
        let parser = AstParser::new().unwrap();
        let result = parser.parse(source, false).unwrap();
        extract_blocks(result.tree.root_node(), source.as_bytes())
    }

    #[test]
    fn extracts_named_function() {
        let blocks = parse_blocks(
            "function greet(name: string): string {\n  const msg = `Hello ${name}`;\n  console.log(msg);\n  return msg;\n}",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "greet");
        assert_eq!(blocks[0].kind, BlockKind::Function);
    }

    #[test]
    fn extracts_arrow_function() {
        let blocks =
            parse_blocks("const compute = (x: number) => {\n  const y = x * 2;\n  const z = y + 1;\n  return z;\n};");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "compute");
        assert_eq!(blocks[0].kind, BlockKind::ArrowFunction);
    }

    #[test]
    fn skips_short_functions() {
        // 2 lines — below the 3-line minimum for block extraction.
        let blocks = parse_blocks("function tiny() { return 1; }");
        assert!(blocks.is_empty(), "expected empty, got: {:?}", blocks);
    }

    #[test]
    fn extracts_class_method() {
        let src = "class Foo {\n  bar(x: number): number {\n    const y = x * 2;\n    const z = y + 1;\n    return z;\n  }\n}";
        let blocks = parse_blocks(src);
        // Should get class + method
        assert!(blocks.len() >= 1, "expected at least 1 block, got {}", blocks.len());
        let methods: Vec<_> = blocks.iter().filter(|b| b.kind == BlockKind::Method).collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "bar");
    }
}
