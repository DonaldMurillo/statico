//! TypeScript file parsing and import resolution.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use rayon::prelude::*;

use crate::parse::AstParser;
use crate::parse::blocks::extract_blocks;
use crate::parse::errors::collect_errors;
use crate::parse::exports::extract_exports;
use crate::parse::metrics::{count_classes, count_functions, count_loc};
use crate::types::*;

use crate::resolution::{Resolver, path_relative_to};

/// Per-file result from parsing.
pub(super) struct FileResult {
    pub rel_path: String,
    pub file_imports: FileImports,
    pub external_specs: Vec<String>,
    pub quality: FileQuality,
    pub loc: usize,
    pub total_lines: usize,
    pub blocks: Vec<crate::parse::blocks::CodeBlock>,
    pub source: String,
    pub dep_targets: Vec<String>,
    pub exports: Vec<String>,
    /// Per-resolved-target imported names: (target_file, vec_of_names).
    pub imported_names: Vec<(String, Vec<String>)>,
}

/// Aggregated results from parsing all source files.
pub(super) type ParseResults = (
    Dependencies,
    Quality,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, usize>,
    BTreeMap<String, usize>,
    BTreeMap<String, Vec<crate::parse::blocks::CodeBlock>>,
    Vec<(String, String)>,
    BTreeMap<String, HashSet<String>>,
);

/// Parse all files in parallel using rayon.
pub fn parse_all_files_parallel(
    root: &Path,
    source_files: &[(String, String)],
    _parser: &AstParser,
    resolver: &Resolver,
    progress: &crate::progress::SharedProgress,
) -> ParseResults {
    use std::sync::Arc;

    // Wrap resolver in Arc for shared access across threads.
    let resolver = Arc::new(resolver.clone());
    let root = root.to_path_buf();

    progress.set_phase("Parsing");

    let results: Vec<Option<FileResult>> = source_files
        .par_iter()
        .map(|(rel_path, _lang)| {
            // Create a new parser per thread (AstParser uses RefCell which is !Sync).
            let parser = match AstParser::new() {
                Ok(p) => p,
                Err(_) => {
                    progress.inc();
                    return None;
                }
            };
            let r = parse_single_file(&root, rel_path, &parser, &resolver);
            progress.inc();
            r
        })
        .collect();

    // Merge results.
    let mut all_imports: Vec<FileImports> = Vec::new();
    let mut all_external: BTreeSet<String> = BTreeSet::new();
    let mut quality_files: Vec<FileQuality> = Vec::new();
    let mut dep_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut file_exports: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut file_loc: BTreeMap<String, usize> = BTreeMap::new();
    let mut file_total_lines: BTreeMap<String, usize> = BTreeMap::new();
    let mut file_blocks: BTreeMap<String, Vec<crate::parse::blocks::CodeBlock>> = BTreeMap::new();
    let mut file_sources: Vec<(String, String)> = Vec::new();
    let mut imported_names: BTreeMap<String, HashSet<String>> = BTreeMap::new();

    for res in results {
        let Some(fr) = res else { continue };
        for ext in fr.external_specs {
            all_external.insert(ext);
        }
        dep_graph.insert(fr.rel_path.clone(), fr.dep_targets);
        file_exports.insert(fr.rel_path.clone(), fr.exports);
        file_loc.insert(fr.rel_path.clone(), fr.loc);
        file_total_lines.insert(fr.rel_path.clone(), fr.total_lines);
        file_blocks.insert(fr.rel_path.clone(), fr.blocks);
        file_sources.push((fr.rel_path.clone(), fr.source));
        all_imports.push(fr.file_imports);
        quality_files.push(fr.quality);
        for (target_key, names) in fr.imported_names {
            imported_names.entry(target_key).or_default().extend(names);
        }
    }

    all_imports.sort_by(|a, b| a.source.cmp(&b.source));
    quality_files.sort_by(|a, b| a.path.cmp(&b.path));

    (
        Dependencies { imports: all_imports, external: all_external.into_iter().collect() },
        Quality { files: quality_files },
        dep_graph,
        file_exports,
        file_loc,
        file_total_lines,
        file_blocks,
        file_sources,
        imported_names,
    )
}

/// Parse a single file and return its result.
pub fn parse_single_file(root: &Path, rel_path: &str, _parser: &AstParser, resolver: &Resolver) -> Option<FileResult> {
    let abs_path = root.join(rel_path);
    let source = std::fs::read_to_string(&abs_path).ok()?;
    let source_for_blocks = source.clone();

    let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
    let is_rust = rel_path.ends_with(".rs");

    if is_rust {
        return super::parse_rust::parse_rust_file(root, rel_path, &abs_path, &source, resolver);
    }

    // Create a fresh parser — the passed-in one is just a placeholder.
    let thread_parser = AstParser::new().ok()?;
    let result = thread_parser.parse(&source, is_tsx)?;
    let root_node = result.tree.root_node();

    // Imports.
    let (internal_specs, external_specs) = crate::parse::imports::extract_imports(root_node, &source);
    let mut dep_targets = resolve_file_imports_static(root, &abs_path, &internal_specs, resolver);

    // Named imports per resolved target (for unused-exports detection).
    let raw_named = crate::parse::imports::extract_named_imports(root_node, &source);
    let file_dir = abs_path.parent().unwrap_or(root);
    let mut imported_names: Vec<(String, Vec<String>)> = Vec::new();
    for (raw_spec, names) in raw_named {
        let resolved = resolver.resolve(file_dir, &raw_spec);
        let target_key = match resolved {
            Some(p) => path_relative_to(root, &p),
            None => raw_spec,
        };
        imported_names.push((target_key, names));
    }

    // Also try resolving "external" specifiers — workspace packages like @mono/ui
    // are classified as external by the import parser but can be resolved to
    // local files through the workspace package mappings.
    let mut resolved_external: Vec<String> = Vec::new();
    for spec in &external_specs {
        if let Some(path) = resolver.resolve(abs_path.parent().unwrap_or(root), spec) {
            let rel = path_relative_to(root, &path);
            if !dep_targets.contains(&rel) {
                dep_targets.push(rel);
            }
            resolved_external.push(spec.clone());
        }
    }
    // Only keep truly external (non-resolved) specs.
    let truly_external: Vec<String> = external_specs.into_iter().filter(|s| !resolved_external.contains(s)).collect();

    // Exports.
    let exports = extract_exports(root_node, &source);

    // Metrics.
    let (loc, total) = count_loc(&source);
    let funcs = count_functions(root_node);
    let classes = count_classes(root_node);
    let cx_metrics = crate::parse::complexity::compute_metrics(root_node, source.as_bytes());

    // Code blocks.
    let blocks = extract_blocks(root_node, source.as_bytes());

    // Parse errors.
    let parse_errors = if result.has_errors {
        collect_errors(root_node, source.as_bytes())
            .into_iter()
            .map(|(msg, line, col)| ParseError { message: msg, line, column: col })
            .collect()
    } else {
        vec![]
    };

    Some(FileResult {
        rel_path: rel_path.to_string(),
        file_imports: FileImports { source: rel_path.to_string(), targets: dep_targets.clone() },
        external_specs: truly_external,
        quality: FileQuality {
            path: rel_path.to_string(),
            metrics: Some(Metrics {
                lines_of_code: loc,
                total_lines: total,
                functions: funcs,
                classes,
                complexity: cx_metrics.complexity,
                max_nesting_depth: cx_metrics.max_nesting_depth,
            }),
            exports: exports.clone(),
            parse_errors,
        },
        loc,
        total_lines: total,
        blocks,
        source: source_for_blocks,
        dep_targets,
        exports,
        imported_names,
    })
}

/// Static version of resolve_file_imports that takes &Resolver (not &mut).
fn resolve_file_imports_static(root: &Path, abs_path: &Path, specs: &[String], resolver: &Resolver) -> Vec<String> {
    let file_dir = abs_path.parent().unwrap_or(root);
    let mut resolved: Vec<String> = Vec::new();
    for spec in specs {
        if let Some(path) = resolver.resolve(file_dir, spec) {
            let rel = path_relative_to(root, &path);
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
