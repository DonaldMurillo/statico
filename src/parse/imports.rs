//! Import extraction from AST.

use std::collections::BTreeMap;

use tree_sitter::Node;

use super::{collect_nodes, unquote};

/// Extract import specifiers from an AST root node.
/// Returns (internal_imports, external_imports).
pub fn extract_imports(root: Node, source: &str) -> (Vec<String>, Vec<String>) {
    let mut internal = Vec::new();
    let mut external = Vec::new();

    // Static import statements: import ... from 'module'
    for node in collect_nodes(root, &["import_statement"]) {
        if let Some(spec) = extract_module_specifier(node, source) {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // Export-from statements: export ... from 'module'
    for node in collect_nodes(root, &["export_statement"]) {
        if let Some(spec) = extract_module_specifier(node, source) {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // Dynamic import() expressions.
    // In tree-sitter, `import('./x')` is a call_expression where the callee
    // is an `import` identifier — there is no `import_expression` node type.
    for call in collect_nodes(root, &["call_expression"]) {
        if let Some(func) = call.child(0)
            && func.kind() == "import"
        {
            if let Some(spec) = extract_dynamic_import_specifier(call, source) {
                classify_import(&spec, &mut internal, &mut external);
            }
        }
    }

    // require() calls.
    for call in collect_nodes(root, &["call_expression"]) {
        if let Some(func) = call.child(0)
            && func.kind() == "identifier"
                && func.utf8_text(source.as_bytes()).unwrap_or("") == "require"
            && let Some(spec) = extract_module_specifier(call, source)
        {
            classify_import(&spec, &mut internal, &mut external);
        }
    }

    // Web Worker pattern: new Worker(new URL('./path', import.meta.url))
    for node in collect_nodes(root, &["new_expression"]) {
        let ctor_id = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "identifier");
        if let Some(ctor) = ctor_id {
            if ctor.utf8_text(source.as_bytes()).unwrap_or("") == "Worker" {
                if let Some(url_spec) = extract_worker_url_arg(node, source) {
                    classify_import(&url_spec, &mut internal, &mut external);
                }
            }
        }
    }

    internal.sort();
    external.sort();
    (internal, external)
}

/// Extract named imports per source specifier (raw, unresolved path).
/// Returns a map from raw module specifier → list of imported names.
/// Covers: `import { a, b } from 'x'`, `import def from 'x'`,
/// `import * as ns from 'x'`, and `export { a } from 'x'`.
pub fn extract_named_imports(root: Node, source: &str) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // Static imports: import ... from 'module'
    for node in collect_nodes(root, &["import_statement"]) {
        if let Some(spec) = extract_module_specifier(node, source) {
            let names = extract_import_names_from_statement(node, source);
            map.entry(spec).or_default().extend(names);
        }
    }

    // Export-from: export { a, b } from 'module'
    for node in collect_nodes(root, &["export_statement"]) {
        let has_from = node.children(&mut node.walk()).any(|c| c.kind() == "from");
        if has_from {
            if let Some(spec) = extract_module_specifier(node, source) {
                let names = collect_export_specifier_names(node, source);
                map.entry(spec).or_default().extend(names);
            }
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Extract the names imported by a single `import_statement` node.
fn extract_import_names_from_statement(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "import_clause" {
            names.extend(extract_names_from_import_clause(child, source));
        }
    }
    names
}

/// Walk an `import_clause` and extract all imported names.
fn extract_names_from_import_clause(clause: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..clause.child_count() {
        let child = clause.child(i).unwrap();
        match child.kind() {
            // Named imports: { foo, bar as baz }
            "named_imports" => {
                for j in 0..child.child_count() {
                    let spec = child.child(j).unwrap();
                    if spec.kind() == "import_specifier" {
                        // The imported name is the first identifier.
                        if let Some(name) = spec.child(0) {
                            if name.kind() == "identifier" {
                                names.push(
                                    name.utf8_text(source.as_bytes())
                                        .unwrap_or("")
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
            }
            // Default import: identifier
            "identifier" => {
                names.push(
                    child.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                );
            }
            // Namespace import: * as identifier
            "namespace_import" => {
                for j in 0..child.child_count() {
                    let c = child.child(j).unwrap();
                    if c.kind() == "identifier" {
                        names.push(c.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

/// Collect names from `export_specifier` children within an export_statement.
fn collect_export_specifier_names(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.kind() == "export_clause" {
            for j in 0..child.child_count() {
                let spec = child.child(j).unwrap();
                if spec.kind() == "export_specifier" {
                    // The first identifier is the original name being re-exported.
                    if let Some(name) = spec.child(0) {
                        if name.kind() == "identifier" {
                            names.push(
                                name.utf8_text(source.as_bytes())
                                    .unwrap_or("")
                                    .to_string(),
                            );
                        }
                    }
                }
            }
        }
    }
    names
}

fn extract_module_specifier(node: Node, source: &str) -> Option<String> {
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
    last_string
}

/// Extract specifier from `import('./module')` dynamic imports.
/// Tree structure: `call_expression` → `arguments` → `string`.
fn extract_dynamic_import_specifier(call: Node, source: &str) -> Option<String> {
    let args = call.children(&mut call.walk()).find(|c| c.kind() == "arguments")?;
    for child in args.children(&mut args.walk()) {
        if child.kind() == "string" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            return Some(unquote(text));
        }
        // Skip template strings with interpolations — can't resolve statically.
        if child.kind() == "template_string" || child.kind() == "template" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            if !text.contains("${") {
                return Some(unquote(text));
            }
        }
    }
    None
}

/// Extract the URL path from a `new Worker(new URL('./path', import.meta.url))` pattern.
fn extract_worker_url_arg(new_expr: Node, source: &str) -> Option<String> {
    let args_node = new_expr
        .children(&mut new_expr.walk())
        .find(|c| c.kind() == "arguments")?;

    for child in args_node.children(&mut args_node.walk()) {
        if child.kind() == "new_expression" {
            let ctor = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")?;
            if ctor.utf8_text(source.as_bytes()).unwrap_or("") == "URL" {
                let url_args = child
                    .children(&mut child.walk())
                    .find(|c| c.kind() == "arguments")?;
                for arg in url_args.children(&mut url_args.walk()) {
                    if arg.kind() == "string" {
                        let text = arg.utf8_text(source.as_bytes()).unwrap_or("");
                        return Some(unquote(text));
                    }
                }
            }
        }
        if child.kind() == "string" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("");
            return Some(unquote(text));
        }
    }
    None
}

fn classify_import(spec: &str, internal: &mut Vec<String>, external: &mut Vec<String>) {
    let is_internal = spec.starts_with('.')
        || spec.starts_with('/')
        || spec.starts_with("@/")
        || spec.starts_with('~')
        || spec.starts_with('#');

    if is_internal {
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

/// Extract the package name from an import specifier.
pub fn extract_package_name(spec: &str) -> String {
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
    use crate::parse::AstParser;

    #[test]
    fn test_extract_imports() {
        let parser = AstParser::new().expect("parser init");
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
    fn test_extract_named_imports() {
        let parser = AstParser::new().expect("parser init");
        let code = r#"
import { foo, bar as baz } from './utils';
import defVal from './config';
import * as mod from './module';
"#;
        let result = parser.parse(code, false).expect("parse");
        let named = extract_named_imports(result.tree.root_node(), code);
        assert_eq!(named.get("./utils").map(|v| v.as_slice()), Some(&["foo".to_string(), "bar".to_string()][..]));
        assert_eq!(named.get("./config").map(|v| v.as_slice()), Some(&["defVal".to_string()][..]));
        assert_eq!(named.get("./module").map(|v| v.as_slice()), Some(&["mod".to_string()][..]));
    }

    #[test]
    fn test_extract_named_imports_export_from() {
        let parser = AstParser::new().expect("parser init");
        let code = r#"export { foo, bar } from './utils';"#;
        let result = parser.parse(code, false).expect("parse");
        let named = extract_named_imports(result.tree.root_node(), code);
        assert_eq!(
            named.get("./utils").map(|v| v.as_slice()),
            Some(&["foo".to_string(), "bar".to_string()][..])
        );
    }

    #[test]
    fn test_extract_package_name() {
        assert_eq!(extract_package_name("lodash"), "lodash");
        assert_eq!(extract_package_name("lodash/merge"), "lodash");
        assert_eq!(extract_package_name("@angular/core"), "@angular/core");
        assert_eq!(extract_package_name("@angular/core/testing"), "@angular/core");
    }

    #[test]
    fn test_web_worker_url_import() {
        let parser = AstParser::new().expect("parser init");
        let code = r#"
const worker = new Worker(new URL('../workers/similarity-worker.ts', import.meta.url));
const w2 = new Worker(new URL('./my-worker.ts', import.meta.url), { type: 'module' });
"#;
        let result = parser.parse(code, false).expect("parse");
        let (internal, external) = extract_imports(result.tree.root_node(), code);
        assert!(internal.iter().any(|s| s.contains("similarity-worker")));
        assert!(internal.iter().any(|s| s.contains("my-worker")));
        assert!(external.is_empty());
    }

    #[test]
    fn test_alias_imports_classified_as_internal() {
        let parser = AstParser::new().expect("parser init");
        let code = r#"
import { foo } from '@/components/foo';
import { bar } from '~/lib/bar';
import { baz } from '#internal/baz';
import { qux } from 'lodash';
"#;
        let result = parser.parse(code, false).expect("parse");
        let (internal, external) = extract_imports(result.tree.root_node(), code);
        assert!(internal.iter().any(|s| s.contains("@/components")));
        assert!(internal.iter().any(|s| s.contains("~/lib")));
        assert!(internal.iter().any(|s| s.contains("#internal")));
        assert!(external.contains(&"lodash".to_string()));
    }
}
