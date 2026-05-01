//! TypeScript language plugin for statico.
//!
//! Handles .ts, .tsx, .js, .jsx files using tree-sitter-typescript.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::languages::{FileAnalysis, LanguagePlugin};
use crate::parse::blocks::extract_blocks;
use crate::parse::errors::collect_errors;
use crate::parse::exports::extract_exports;
use crate::parse::metrics::{count_classes, count_functions, count_loc};
use crate::resolution::Resolver;
use crate::types::ParseError;

/// TypeScript (and JavaScript) language plugin.
///
/// Caches the resolver per project root to avoid re-reading tsconfig
/// and workspace package manifests for every file.
pub struct TypeScriptPlugin {
    resolver_cache: Mutex<Vec<(PathBuf, Arc<Resolver>)>>,
}

impl TypeScriptPlugin {
    pub fn new() -> Self {
        Self { resolver_cache: Mutex::new(Vec::new()) }
    }

    fn get_resolver(&self, root: &Path) -> Arc<Resolver> {
        // V7-2: Recover from poisoned mutex (another thread panicked while holding it).
        let mut cache = self.resolver_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, resolver)) = cache.iter().find(|(r, _)| r == root) {
            return Arc::clone(resolver);
        }
        let resolver = Arc::new(build_resolver(root));
        cache.push((root.to_path_buf(), Arc::clone(&resolver)));
        resolver
    }
}

impl Default for TypeScriptPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for TypeScriptPlugin {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn name(&self) -> &str {
        "typescript"
    }

    fn analyze_file(&self, root: &Path, rel_path: &str, source: &str) -> Option<FileAnalysis> {
        let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");

        // Parse with tree-sitter-typescript.
        let parser = crate::parse::AstParser::new().ok()?;
        let result = parser.parse(source, is_tsx)?;
        let root_node = result.tree.root_node();

        let resolver = self.get_resolver(root);
        let abs_path = root.join(rel_path);
        let file_dir = abs_path.parent().unwrap_or(root);

        // Imports.
        let (internal_specs, external_specs) = crate::parse::imports::extract_imports(root_node, source);
        let mut dep_targets = resolve_specs(root, file_dir, &internal_specs, &resolver);

        // Named imports per resolved target.
        let raw_named = crate::parse::imports::extract_named_imports(root_node, source);
        let mut imported_names: Vec<(String, Vec<String>)> = Vec::new();
        for (raw_spec, names) in raw_named {
            let resolved = resolver.resolve(file_dir, &raw_spec);
            let target_key = match resolved {
                Some(p) => crate::resolution::path_relative_to(root, &p),
                None => raw_spec,
            };
            imported_names.push((target_key, names));
        }

        // Resolve workspace-external specs that are actually local (e.g., @mono/ui).
        let mut resolved_external: Vec<String> = Vec::new();
        for spec in &external_specs {
            if let Some(path) = resolver.resolve(file_dir, spec) {
                let rel = crate::resolution::path_relative_to(root, &path);
                if !dep_targets.contains(&rel) {
                    dep_targets.push(rel);
                }
                resolved_external.push(spec.clone());
            }
        }
        let truly_external: Vec<String> =
            external_specs.into_iter().filter(|s| !resolved_external.contains(s)).collect();

        // Exports.
        let exports = extract_exports(root_node, source);

        // Metrics.
        let (loc, total) = count_loc(source);
        let funcs = count_functions(root_node);
        let classes = count_classes(root_node);
        let cx = crate::parse::complexity::compute_metrics(root_node, source.as_bytes());
        let blocks = extract_blocks(root_node, source.as_bytes());

        let parse_errors = if result.has_errors {
            collect_errors(root_node, source.as_bytes())
                .into_iter()
                .map(|(msg, line, col)| ParseError { message: msg, line, column: col })
                .collect()
        } else {
            vec![]
        };

        Some(FileAnalysis {
            rel_path: rel_path.to_string(),
            dep_targets,
            external_specs: truly_external,
            imported_names,
            exports,
            loc,
            total_lines: total,
            functions: funcs,
            classes,
            complexity: cx.complexity,
            max_nesting_depth: cx.max_nesting_depth,
            parse_errors,
            blocks,
            source: source.to_string(),
        })
    }

    fn config_files(&self) -> &[&str] {
        &["tsconfig.json", "jsconfig.json", "package.json"]
    }

    fn skip_dirs(&self) -> &[&str] {
        &["node_modules", ".next", ".nuxt", "dist", "build", "out"]
    }

    fn should_skip_file(&self, rel_path: &str) -> bool {
        rel_path.ends_with(".d.ts")
    }

    fn resolve_import(&self, root: &Path, from_file: &str, spec: &str) -> Option<String> {
        let resolver = self.get_resolver(root);
        let abs_path = root.join(from_file);
        let file_dir = abs_path.parent().unwrap_or(root);
        resolver.resolve(file_dir, spec).map(|p| crate::resolution::path_relative_to(root, &p))
    }
}

/// Build a resolver with tsconfig path aliases and workspace packages loaded.
fn build_resolver(root: &Path) -> Resolver {
    let mut resolver = Resolver::new(root);

    for tsconfig in &["tsconfig.json", "tsconfig.app.json"] {
        let path = root.join(tsconfig);
        if path.exists() {
            resolver.load_tsconfig_paths(&path);
        }
    }

    resolver.load_workspace_packages();
    resolver.load_all_tsconfig_paths();

    resolver
}

/// Resolve import specifiers to relative file paths.
fn resolve_specs(
    root: &Path,
    file_dir: &Path,
    specs: &[String],
    resolver: &Resolver,
) -> Vec<String> {
    let mut resolved: Vec<String> = Vec::new();
    for spec in specs {
        if let Some(path) = resolver.resolve(file_dir, spec) {
            let rel = crate::resolution::path_relative_to(root, &path);
            if !resolved.contains(&rel) {
                resolved.push(rel);
            }
        } else if !resolved.contains(spec) {
            resolved.push(spec.clone());
        }
    }
    resolved.sort();
    resolved
}
