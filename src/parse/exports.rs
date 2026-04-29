//! Export name extraction from AST.

use tree_sitter::Node;

use super::collect_nodes;

/// Extract the names of all exports from an AST root node.
/// Skips `export * from '...'` re-exports.
pub fn extract_exports(root: Node, source: &str) -> Vec<String> {
    let mut exports: Vec<String> = Vec::new();

    for node in collect_nodes(root, &["export_statement"]) {
        exports.extend(extract_all_from_export(node, source));
    }

    exports
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn find_named_decl_child(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        match child.kind() {
            "function_declaration" | "generator_function_declaration" | "class_declaration"
            | "type_alias_declaration" | "interface_declaration" => {
                return first_identifier_name(child, source);
            }
            "lexical_declaration" | "variable_declaration" => {
                // Return the first declarator name. All names are collected
                // by the caller when needed (see below).
                return first_declarator_name(child, source);
            }
            _ => {}
        }
    }
    None
}

/// Extract all export names from a single export_statement node,
/// handling multi-declarator `export const a = 1, b = 2;`.
fn extract_all_from_export(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut found_default = false;
    let mut found_star = false;
    let mut found_from = false;

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        match child.kind() {
            "default" => found_default = true,
            "from" => found_from = true,
            "*" => found_star = true,
            _ => {}
        }
    }

    // export * from '...' — skip.
    if found_star && found_from {
        return names;
    }

    if found_default {
        if let Some(name) = find_named_decl_child(node, source) {
            names.push(name);
        } else {
            names.push("default".to_string());
        }
        return names;
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        match child.kind() {
            "function_declaration" | "generator_function_declaration" | "class_declaration"
            | "type_alias_declaration" | "interface_declaration" => {
                if let Some(name) = first_identifier_name(child, source) {
                    names.push(name);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                names.extend(all_declarator_names(child, source));
            }
            "export_clause" => {
                names.extend(collect_export_specifiers(child, source));
            }
            _ => {}
        }
    }

    names
}

fn first_identifier_name(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "type_identifier" || child.kind() == "identifier" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

fn first_declarator_name(node: Node, source: &str) -> Option<String> {
    // Return the first declarator name for backwards compatibility.
    // Callers that need all names should use collect_declarator_names.
    all_declarator_names(node, source).into_iter().next()
}

fn all_declarator_names(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "variable_declarator" {
            if let Some(id) = child.child(0) {
                if id.kind() == "identifier" {
                    names.push(id.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
        }
    }
    names
}

fn collect_export_specifiers(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "export_specifier" {
            // export_specifier children: identifier "as" identifier
            // The exported name is the last identifier (the alias if present).
            let mut last_id: Option<String> = None;
            for j in 0..child.child_count() {
                let c = child.child(j).unwrap();
                if c.kind() == "identifier" {
                    last_id = Some(c.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            if let Some(name) = last_id {
                names.push(name);
            }
        }
    }
    names
}

/// Extract all exported type/interface declarations.
/// Returns `(name, kind)` pairs where kind is "type" or "interface".
pub fn extract_type_exports(root: Node, source: &str) -> Vec<(String, String)> {
    let mut types: Vec<(String, String)> = Vec::new();

    for node in collect_nodes(root, &["export_statement"]) {
        // Check if this export contains a type or interface declaration.
        for i in 0..node.child_count() {
            let child = node.child(i).unwrap();
            match child.kind() {
                "type_alias_declaration" => {
                    if let Some(name) = first_identifier_name(child, source) {
                        types.push((name, "type".to_string()));
                    }
                }
                "interface_declaration" => {
                    if let Some(name) = first_identifier_name(child, source) {
                        types.push((name, "interface".to_string()));
                    }
                }
                _ => {}
            }
        }
    }

    types
}

/// Extract type names from re-export statements.
///
/// Handles:
/// - `export type { X, Y } from './mod'` — all names included (type-only)
/// - `export { PascalCase, lowercase } from './mod'` — PascalCase names included
/// - `export { default as Foo } from './mod'` — renamed exports included
///
/// Returns `(name, kind)` pairs where kind is "type" for type-only re-exports
/// or "reexport" for non-type-only re-exports.
pub fn extract_reexport_types(root: Node, source: &str) -> Vec<(String, String)> {
    let mut types: Vec<(String, String)> = Vec::new();

    for node in collect_nodes(root, &["export_statement"]) {
        let mut has_from = false;
        let mut is_type_only = false;
        let mut has_star = false;
        let mut export_clause: Option<Node> = None;

        for i in 0..node.child_count() {
            let child = node.child(i).unwrap();
            match child.kind() {
                "from" => has_from = true,
                "type" => is_type_only = true,
                "*" => has_star = true,
                "export_clause" => export_clause = Some(child),
                _ => {}
            }
        }

        // Only process named re-exports (must have 'from' and no '*')
        if !has_from || has_star {
            continue;
        }

        let Some(clause) = export_clause else {
            continue;
        };

        for i in 0..clause.child_count() {
            let child = clause.child(i).unwrap();
            if child.kind() == "export_specifier" {
                if let Some(name) = get_last_identifier(child, source) {
                    if is_type_only {
                        types.push((name, "type".to_string()));
                    } else if is_pascal_case(&name) {
                        types.push((name, "reexport".to_string()));
                    }
                }
            }
        }
    }

    types
}

/// Extract module specifiers from `export * from './mod'` statements.
pub fn extract_star_reexport_specs(root: Node, source: &str) -> Vec<String> {
    let mut specs: Vec<String> = Vec::new();

    for node in collect_nodes(root, &["export_statement"]) {
        let mut has_from = false;
        let mut has_star = false;
        let mut module_spec: Option<String> = None;

        for i in 0..node.child_count() {
            let child = node.child(i).unwrap();
            match child.kind() {
                "from" => has_from = true,
                "*" => has_star = true,
                "string" => {
                    let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                    module_spec = Some(super::unquote(text));
                }
                _ => {}
            }
        }

        if has_star && has_from {
            if let Some(spec) = module_spec {
                specs.push(spec);
            }
        }
    }

    specs
}

/// Get the last identifier child of a node (handles `Foo as Bar` aliases).
fn get_last_identifier(node: Node, source: &str) -> Option<String> {
    let mut last_id: Option<String> = None;
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "identifier" {
            last_id = Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    last_id
}

/// Check if a name starts with an uppercase letter (PascalCase heuristic).
fn is_pascal_case(name: &str) -> bool {
    name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::AstParser;

    #[test]
    fn named_functions() {
        let mut p = AstParser::new().unwrap();
        let code = "export function foo() {}\nexport function bar() {}";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"foo".into()));
        assert!(e.contains(&"bar".into()));
    }

    #[test]
    fn const_exports() {
        let mut p = AstParser::new().unwrap();
        let code = "export const A = 1;\nexport const B = 2, C = 3;";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"A".into()));
        assert!(e.contains(&"B".into()));
        assert!(e.contains(&"C".into()));
    }

    #[test]
    fn class_export() {
        let mut p = AstParser::new().unwrap();
        let code = "export class MyClass {}";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"MyClass".into()));
    }

    #[test]
    fn type_and_interface() {
        let mut p = AstParser::new().unwrap();
        let code = "export type Result<T> = { ok: T } | { err: string };\nexport interface Config { debug: boolean }";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"Result".into()));
        assert!(e.contains(&"Config".into()));
    }

    #[test]
    fn named_list() {
        let mut p = AstParser::new().unwrap();
        let code = "export { foo, bar, baz };";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"foo".into()));
        assert!(e.contains(&"bar".into()));
        assert!(e.contains(&"baz".into()));
    }

    #[test]
    fn default_function() {
        let mut p = AstParser::new().unwrap();
        let code = "export default function main() {}";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"main".into()));
        assert!(!e.contains(&"default".into()));
    }

    #[test]
    fn default_expression() {
        let mut p = AstParser::new().unwrap();
        let code = "export default 42;";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"default".into()));
    }

    #[test]
    fn reexport_star_ignored() {
        let mut p = AstParser::new().unwrap();
        let code = "export * from './utils';\nexport function local() {}";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert_eq!(e.len(), 1);
        assert!(e.contains(&"local".into()));
    }

    #[test]
    fn alias() {
        let mut p = AstParser::new().unwrap();
        let code = "export { foo as bar };";
        let r = p.parse(code, false).unwrap();
        let e = extract_exports(r.tree.root_node(), code);
        assert!(e.contains(&"bar".into()));
        assert!(!e.contains(&"foo".into()));
    }

    #[test]
    fn reexport_type_only() {
        let mut p = AstParser::new().unwrap();
        let code = "export type { Foo, Bar } from './utils';";
        let r = p.parse(code, false).unwrap();
        let types = extract_reexport_types(r.tree.root_node(), code);
        assert_eq!(types.len(), 2);
        assert!(types.contains(&("Foo".into(), "type".into())));
        assert!(types.contains(&("Bar".into(), "type".into())));
    }

    #[test]
    fn reexport_mixed() {
        let mut p = AstParser::new().unwrap();
        // PascalCase names from non-type-only re-exports are included
        let code = "export { ImageItem, TagOption, helperFn } from './types';";
        let r = p.parse(code, false).unwrap();
        let types = extract_reexport_types(r.tree.root_node(), code);
        assert_eq!(types.len(), 2); // ImageItem + TagOption, not helperFn
        assert!(types.contains(&("ImageItem".into(), "reexport".into())));
        assert!(types.contains(&("TagOption".into(), "reexport".into())));
    }

    #[test]
    fn reexport_with_alias() {
        let mut p = AstParser::new().unwrap();
        let code = "export { default as MyDefault } from './mod';";
        let r = p.parse(code, false).unwrap();
        let types = extract_reexport_types(r.tree.root_node(), code);
        assert_eq!(types.len(), 1);
        assert!(types.contains(&("MyDefault".into(), "reexport".into())));
    }

    #[test]
    fn reexport_star_specs() {
        let mut p = AstParser::new().unwrap();
        let code = "export * from './utils';\nexport * from '../types';\nexport { Foo } from './bar';";
        let r = p.parse(code, false).unwrap();
        let specs = extract_star_reexport_specs(r.tree.root_node(), code);
        assert_eq!(specs, vec!["./utils".to_string(), "../types".to_string()]);
    }

    #[test]
    fn reexport_no_from_ignored() {
        let mut p = AstParser::new().unwrap();
        // export { Foo } without 'from' is a regular re-export, not from another module
        let code = "const Foo = 1;\nexport { Foo };";
        let r = p.parse(code, false).unwrap();
        let types = extract_reexport_types(r.tree.root_node(), code);
        assert!(types.is_empty());
    }

    #[test]
    fn type_exports_extraction() {
        let mut p = AstParser::new().unwrap();
        let code = "export type Result<T> = { ok: T } | { err: string };\nexport interface Config { debug: boolean }\nexport function foo() {}";
        let r = p.parse(code, false).unwrap();
        let type_exports = extract_type_exports(r.tree.root_node(), code);
        assert_eq!(type_exports.len(), 2);
        assert!(type_exports.contains(&("Result".to_string(), "type".to_string())));
        assert!(type_exports.contains(&("Config".to_string(), "interface".to_string())));
    }
}
