//! Rust-specific parsing: extract exports (pub items) and imports (use statements).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use tree_sitter::{Node, Parser, Tree};

/// Wrapper around tree-sitter for Rust parsing.
pub struct RustAstParser {
    parser: std::sync::Mutex<Parser>,
}

impl RustAstParser {
    pub fn new() -> Result<Self, tree_sitter::LanguageError> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang)?;
        Ok(Self { parser: std::sync::Mutex::new(parser) })
    }

    pub fn parse(&self, code: &str) -> Option<RustParseResult> {
        // V7-2: Recover from poisoned mutex (another thread panicked while holding it).
        let mut parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        let tree = parser.parse(code, None)?;
        Some(RustParseResult { tree })
    }
}

pub struct RustParseResult {
    pub tree: Tree,
}

// ---------------------------------------------------------------------------
// Export extraction
// ---------------------------------------------------------------------------

/// A Rust exported (public) item.
#[derive(Debug, Clone)]
pub struct RustExport {
    /// Name of the exported item (function name, struct name, etc.).
    pub name: String,
    /// Type of the export: "function", "struct", "enum", "trait", "module", "const", "type", "static".
    pub kind: String,
}

/// Extract all `pub` items from a Rust source file.
pub fn extract_exports(code: &str, tree: &Tree) -> Vec<RustExport> {
    let mut exports = Vec::new();
    let root = tree.root_node();

    // Item types that can be pub
    let item_kinds = [
        ("function_item", "function"),
        ("struct_item", "struct"),
        ("enum_item", "enum"),
        ("trait_item", "trait"),
        ("mod_item", "module"),
        ("const_item", "const"),
        ("type_item", "type"),
        ("static_item", "static"),
    ];

    collect_pub_items(root, code, &item_kinds, &mut exports);
    exports
}

fn collect_pub_items(node: Node, code: &str, item_kinds: &[(&str, &str)], exports: &mut Vec<RustExport>) {
    for child in node.children(&mut node.walk()) {
        for (tree_kind, export_kind) in item_kinds {
            if child.kind() == *tree_kind
                && is_pub(&child)
                && let Some(name) = get_item_name(&child, code)
            {
                exports.push(RustExport { name, kind: export_kind.to_string() });
            }
        }
        // Recurse into impl blocks and mod blocks for inline items
        if child.kind() == "impl_item" || child.kind() == "declaration_list" {
            collect_pub_items(child, code, item_kinds, exports);
        }
    }
}

/// Check if a node has a `pub` visibility modifier.
fn is_pub(node: &Node) -> bool {
    // First child that is a visibility_modifier containing "pub"
    for child in node.children(&mut node.walk()) {
        if child.kind() == "visibility_modifier" {
            return true;
        }
    }
    false
}

/// Get the name identifier of an item node.
fn get_item_name(node: &Node, code: &str) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    Some(code[name_node.byte_range()].to_string())
}

// ---------------------------------------------------------------------------
// Import extraction
// ---------------------------------------------------------------------------

/// A Rust import (use statement).
#[derive(Debug, Clone)]
pub struct RustImport {
    /// The imported name(s) — e.g., "HashMap", "Deserialize".
    pub names: Vec<String>,
    /// The raw path string — e.g., "std::collections::HashMap", "crate::parser".
    pub raw_path: String,
    /// Whether this is a glob import (use foo::*).
    pub is_glob: bool,
}

/// Extract all `use` declarations from a Rust source file.
pub fn extract_imports(code: &str, tree: &Tree) -> Vec<RustImport> {
    let mut imports = Vec::new();
    let root = tree.root_node();

    for node in root.children(&mut root.walk()) {
        if node.kind() == "use_declaration"
            && let Some(imp) = parse_use_decl(node, code)
        {
            imports.extend(imp);
        }
    }
    imports
}

fn parse_use_decl(node: Node, code: &str) -> Option<Vec<RustImport>> {
    // The child after "use" keyword is the path expression
    let path_node = node.child(1)?;
    let results = extract_from_use_node(path_node, code);
    if results.is_empty() { None } else { Some(results) }
}

/// Recursively extract imports from a use declaration's path node.
fn extract_from_use_node(node: Node, code: &str) -> Vec<RustImport> {
    match node.kind() {
        "scoped_identifier" => {
            // e.g., std::collections::HashMap or crate::parser::AstParser
            let full_path = collect_scoped_path(node, code);
            let last = full_path.split("::").last().unwrap_or("").to_string();
            vec![RustImport { names: vec![last], raw_path: full_path, is_glob: false }]
        }
        "scoped_use_list" => {
            // e.g., std::collections::{HashMap, BTreeMap} or serde::{Deserialize, Serialize}
            let children: Vec<Node> = node.children(&mut node.walk()).collect();
            let prefix_path = children
                .iter()
                .find(|c| c.kind() == "scoped_identifier" || c.kind() == "identifier")
                .map(|c| {
                    if c.kind() == "scoped_identifier" {
                        collect_scoped_path(*c, code)
                    } else {
                        code[c.byte_range()].to_string()
                    }
                })
                .unwrap_or_default();

            let use_list = children.iter().find(|c| c.kind() == "use_list");

            if let Some(list) = use_list {
                let names = collect_use_list_names(*list, code);
                vec![RustImport { names, raw_path: prefix_path, is_glob: false }]
            } else {
                vec![]
            }
        }
        "use_list" => {
            // Top-level use list without prefix: use {foo, bar};
            let names = collect_use_list_names(node, code);
            vec![RustImport { names, raw_path: String::new(), is_glob: false }]
        }
        "use_wildcard" => {
            // use std::sync::*;
            let scoped = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "scoped_identifier")
                .map(|c| collect_scoped_path(c, code))
                .unwrap_or_default();
            vec![RustImport { names: vec!["*".to_string()], raw_path: scoped, is_glob: true }]
        }
        "identifier" => {
            // Simple: use foo;
            let name = code[node.byte_range()].to_string();
            vec![RustImport { names: vec![name.clone()], raw_path: name, is_glob: false }]
        }
        _ => vec![],
    }
}

/// Collect the full scoped path from a scoped_identifier node (e.g., "std::collections::HashMap").
fn collect_scoped_path(node: Node, code: &str) -> String {
    let mut parts = Vec::new();
    collect_scoped_parts(node, code, &mut parts);
    parts.join("::")
}

fn collect_scoped_parts(node: Node, code: &str, parts: &mut Vec<String>) {
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "scoped_identifier" => {
                collect_scoped_parts(child, code, parts);
            }
            "identifier" | "crate" | "super" | "self" => {
                parts.push(code[child.byte_range()].to_string());
            }
            _ => {}
        }
    }
}

/// Collect names from a use_list node: {foo, bar, Baz}.
fn collect_use_list_names(node: Node, code: &str) -> Vec<String> {
    node.children(&mut node.walk())
        .filter(|c| c.kind() == "identifier" || c.kind() == "scoped_identifier" || c.kind() == "self")
        .map(|c| {
            if c.kind() == "scoped_identifier" {
                // Nested path like Foo::Bar — take last segment
                let path = collect_scoped_path(c, code);
                path.split("::").last().unwrap_or("").to_string()
            } else {
                code[c.byte_range()].to_string()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Module declaration extraction (mod foo;)
// ---------------------------------------------------------------------------

/// A Rust module declaration (`mod foo;` or `mod foo { ... }`).
#[derive(Debug, Clone)]
pub struct RustModDecl {
    /// Module name.
    pub name: String,
    /// Whether this is an inline module (has a body `{ ... }`).
    pub is_inline: bool,
    /// Override path from #[path = "..."] attribute.
    pub path_override: Option<String>,
}

/// Extract all `mod` declarations from a Rust source file.
/// These are the "local imports" in Rust — they declare submodules.
pub fn extract_mod_decls(code: &str, tree: &Tree) -> Vec<RustModDecl> {
    let mut mods = Vec::new();
    let root = tree.root_node();
    collect_mod_decls(root, code, &mut mods);
    mods
}

fn collect_mod_decls(node: Node, code: &str, mods: &mut Vec<RustModDecl>) {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "mod_item" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = code[name_node.byte_range()].to_string();
                // Check if it has a body (inline module) — look for declaration_list child
                let has_body = child.children(&mut child.walk()).any(|c| c.kind() == "declaration_list");
                // Check for #[path = "..."] attribute
                let path_override = extract_path_attr(&child, code);
                mods.push(RustModDecl { name, is_inline: has_body, path_override });
            }
        } else if child.kind() == "macro_invocation" {
            // Macro bodies like cfg_feature! { pub mod client; } or
            // cfg_proto! { mod headers; mod proto; }
            // Scan token_tree children for `mod <ident>;` patterns.
            extract_mods_from_macro(&child, code, mods);
        }
        // Don't recurse into mod_item children
        if child.kind() != "mod_item" {
            collect_mod_decls(child, code, mods);
        }
    }
}

/// Extract `mod` declarations from inside macro invocations.
/// Macro bodies like `cfg_feature! { pub mod client; }` contain `mod` as
/// bare tokens inside a `token_tree` node, not as `mod_item` AST nodes.
/// Also handles nested macros: `cfg_net! { cfg_inner! { mod foo; } }`.
fn extract_mods_from_macro(macro_node: &Node, code: &str, mods: &mut Vec<RustModDecl>) {
    // Find the token_tree child (the {...} body)
    let Some(token_tree) = macro_node.children(&mut macro_node.walk()).find(|c| c.kind() == "token_tree") else {
        return;
    };

    scan_token_tree_for_mods(&token_tree, code, mods);
}

/// Recursively scan a token_tree node for `mod <ident>;` patterns,
/// also descending into nested token_tree children (nested macros).
fn scan_token_tree_for_mods(token_tree: &Node, code: &str, mods: &mut Vec<RustModDecl>) {
    let tokens: Vec<Node> = token_tree.children(&mut token_tree.walk()).collect();
    scan_tokens_with_prefix(&tokens, code, mods, &[]);
}

fn scan_tokens_with_prefix(tokens: &[Node], code: &str, mods: &mut Vec<RustModDecl>, prefix: &[String]) {
    let mut i = 0;
    let inline_mod_stack: Vec<String> = prefix.to_vec();
    while i < tokens.len() {
        let tok = &tokens[i];
        // Recurse into nested token_tree nodes (for macros and attributes)
        if tok.kind() == "token_tree" {
            scan_token_tree_for_mods(tok, code, mods);
            i += 1;
            continue;
        }
        // Look for `#` which might start an attribute like #[path = "..."]
        if &code[tok.byte_range()] == "#" && i + 1 < tokens.len() && tokens[i + 1].kind() == "token_tree" {
            let attr_path = extract_path_from_token_attr(&tokens[i + 1], code);
            if attr_path.is_some() {
                let mut j = i + 2;
                while j + 1 < tokens.len()
                    && &code[tokens[j].byte_range()] == "#"
                    && tokens[j + 1].kind() == "token_tree"
                {
                    j += 2;
                }
                if j + 2 < tokens.len() && &code[tokens[j].byte_range()] == "mod" {
                    let name_tok = &tokens[j + 1];
                    let sep_tok = &tokens[j + 2];
                    if name_tok.kind() == "identifier" {
                        let name = code[name_tok.byte_range()].to_string();
                        let _sep = &code[sep_tok.byte_range()];
                        let full_prefix =
                            if inline_mod_stack.is_empty() { None } else { Some(inline_mod_stack.join("/")) };
                        let full_name = match full_prefix {
                            Some(p) => format!("{}/{}", p, name),
                            None => name,
                        };
                        mods.push(RustModDecl { name: full_name, is_inline: false, path_override: attr_path });
                        i = j + 3;
                        continue;
                    }
                }
            }
        }
        // Look for `mod` keyword followed by identifier
        if &code[tok.byte_range()] == "mod" && i + 2 < tokens.len() {
            let name_tok = &tokens[i + 1];
            let sep_tok = &tokens[i + 2];
            if name_tok.kind() == "identifier" {
                let name = code[name_tok.byte_range()].to_string();
                // Inline module if next is token_tree ({...}) or literal "{" followed by content
                let is_inline = sep_tok.kind() == "token_tree" || &code[sep_tok.byte_range()] == "{";
                if is_inline {
                    // Inline module — recurse into its body with the name as prefix
                    let mut child_stack = inline_mod_stack.clone();
                    child_stack.push(name);
                    if sep_tok.kind() == "token_tree" {
                        let inner: Vec<Node> = sep_tok.children(&mut sep_tok.walk()).collect();
                        scan_tokens_with_prefix(&inner, code, mods, &child_stack);
                    }
                    i += 3;
                    continue;
                } else if &code[sep_tok.byte_range()] == ";" {
                    // mod <name>; — file reference
                    let full_prefix = if inline_mod_stack.is_empty() { None } else { Some(inline_mod_stack.join("/")) };
                    let full_name = match full_prefix {
                        Some(p) => format!("{}/{}", p, name),
                        None => name,
                    };
                    mods.push(RustModDecl { name: full_name, is_inline: false, path_override: None });
                    i += 3;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// Extract path value from an attribute token_tree like `[path = "value"]`.
fn extract_path_from_token_attr(attr_tt: &Node, code: &str) -> Option<String> {
    // attr_tt is a token_tree that starts with `[`
    let tokens: Vec<Node> = attr_tt.children(&mut attr_tt.walk()).collect();
    // Look for: [ path = "value" ]
    let mut i = 0;
    while i + 3 < tokens.len() {
        if &code[tokens[i].byte_range()] == "path"
            && &code[tokens[i + 1].byte_range()] == "="
            && tokens[i + 2].kind() == "string_literal"
        {
            let raw = &code[tokens[i + 2].byte_range()];
            // Strip quotes
            if raw.len() >= 2 {
                return Some(raw[1..raw.len() - 1].to_string());
            }
        }
        i += 1;
    }
    None
}

/// Extract `#[path = "..."]` attribute from a mod_item node.
/// The attribute is a preceding sibling, not a child of mod_item.
fn extract_path_attr(node: &Node, code: &str) -> Option<String> {
    // Walk preceding siblings to find attribute_item with #[path = ...]
    let parent = node.parent()?;
    let mut last_path: Option<String> = None;
    for child in parent.children(&mut parent.walk()) {
        if child.id() == node.id() {
            break;
        }
        if child.kind() == "attribute_item" {
            let attr_text = &code[child.byte_range()];
            if let Some(val) = parse_path_attr_value(attr_text) {
                last_path = Some(val);
            }
        } else if child.kind() != "attribute_item" && child.kind() != "inner_attribute_item" {
            // Reset if there's a non-attribute node between the attribute and mod_item
            last_path = None;
        }
    }
    last_path
}

/// Parse `#[path = "value"]` from attribute text.
fn parse_path_attr_value(attr: &str) -> Option<String> {
    // Strip #[ and ]
    let inner = attr.trim().trim_start_matches("#[").trim_end_matches(']');
    // Look for path = "..."
    if !inner.starts_with("path") {
        return None;
    }
    let rest = inner[4..].trim();
    if !rest.starts_with('=') {
        return None;
    }
    let val = rest[1..].trim();
    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
        Some(val[1..val.len() - 1].to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(code: &str) -> Tree {
        let parser = RustAstParser::new().unwrap();
        parser.parse(code).unwrap().tree
    }

    #[test]
    fn test_extract_pub_functions() {
        let code = r#"
pub fn hello() {}
fn private() {}
pub fn world(arg: &str) -> String { arg.to_string() }
"#;
        let tree = parse(code);
        let exports = extract_exports(code, &tree);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["hello", "world"]);
        assert_eq!(exports[0].kind, "function");
    }

    #[test]
    fn test_extract_pub_structs_enums() {
        let code = r#"
pub struct User { name: String }
struct Internal {}
pub enum Status { Active, Inactive }
"#;
        let tree = parse(code);
        let exports = extract_exports(code, &tree);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"User"));
        assert!(names.contains(&"Status"));
        assert!(!names.contains(&"Internal"));
    }

    #[test]
    fn test_extract_pub_traits() {
        let code = r#"
pub trait Draw { fn render(&self); }
trait InternalTrait {}
"#;
        let tree = parse(code);
        let exports = extract_exports(code, &tree);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Draw"]);
    }

    #[test]
    fn test_extract_pub_const_and_type() {
        let code = r#"
pub const MAX: usize = 100;
const PRIVATE: i32 = 42;
pub type Result<T> = std::result::Result<T, Error>;
"#;
        let tree = parse(code);
        let exports = extract_exports(code, &tree);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"MAX"));
        assert!(names.contains(&"Result"));
        assert!(!names.contains(&"PRIVATE"));
    }

    #[test]
    fn test_extract_simple_imports() {
        let code = r#"
use std::collections::HashMap;
use crate::parser::AstParser;
use super::sibling_mod::SomeType;
"#;
        let tree = parse(code);
        let imports = extract_imports(code, &tree);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].names, vec!["HashMap"]);
        assert_eq!(imports[0].raw_path, "std::collections::HashMap");
        assert_eq!(imports[1].names, vec!["AstParser"]);
        assert_eq!(imports[1].raw_path, "crate::parser::AstParser");
        assert_eq!(imports[2].names, vec!["SomeType"]);
    }

    #[test]
    fn test_extract_grouped_imports() {
        let code = r#"use serde::{Deserialize, Serialize};"#;
        let tree = parse(code);
        let imports = extract_imports(code, &tree);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].names, vec!["Deserialize", "Serialize"]);
        assert_eq!(imports[0].raw_path, "serde");
    }

    #[test]
    fn test_extract_glob_imports() {
        let code = r#"use std::sync::*;"#;
        let tree = parse(code);
        let imports = extract_imports(code, &tree);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_glob);
        assert_eq!(imports[0].raw_path, "std::sync");
    }

    #[test]
    fn test_extract_self_import() {
        let code = r#"use std::io::{self, Read, Write};"#;
        let tree = parse(code);
        let imports = extract_imports(code, &tree);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].names, vec!["self", "Read", "Write"]);
    }
}
