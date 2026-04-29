//! Main analysis orchestrator. Coordinates parsing, discovery, and issue detection.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;

use crate::parse::blocks::extract_blocks;
use crate::parse::metrics::{count_loc, count_functions, count_classes};
use crate::parse::exports::extract_exports;
use crate::parse::errors::collect_errors;
use crate::parse::AstParser;
use crate::types::*;

use crate::discovery::{discover_source_files, discover_config_files, discover_entry_points};
use crate::issues::detect_issues;
use crate::resolution::{Resolver, path_relative_to};


/// Run the full analysis on a project directory.
pub fn analyze(root: &Path) -> Result<AnalysisOutput, String> {
    if !root.exists() {
        return Err(format!("path not found: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }

    let parser = AstParser::new().map_err(|e| format!("failed to init parser: {}", e))?;

    let source_files = discover_source_files(root)?;
    let config_files = discover_config_files(root);
    let entry = discover_entry_points(root, &source_files);

    let entry_points: Vec<String> = entry.framework.iter().cloned().collect();
    let implicit_entries: Vec<String> = entry.implicit.iter().cloned().collect();
    let mut public_api: Vec<String> = entry.public_api.iter().cloned().collect();
    let all_entries: Vec<String> = entry.all().into_iter().collect();

    // Auto-detect published packages under packages/ — their source files
    // are public API. Published packages expose their exports for external
    // consumers, so unused exports within the monorepo are expected.
    let published_dirs = crate::resolution::find_published_package_dirs(root);
    if !published_dirs.is_empty() {
        for (rel_path, _lang) in &source_files {
            for pkg_dir in &published_dirs {
                if rel_path.starts_with(pkg_dir.as_str())
                    || rel_path.starts_with(&format!("{}/", pkg_dir))
                {
                    public_api.push(rel_path.clone());
                    break;
                }
            }
        }
    }

    // Build resolver with tsconfig path aliases and workspace package mappings.
    let mut resolver = Resolver::new(root);
    let tsconfig_path = root.join("tsconfig.json");
    if tsconfig_path.exists() {
        resolver.load_tsconfig_paths(&tsconfig_path);
    }
    // Also try tsconfig.app.json (Next.js 15+ splits these).
    let tsconfig_app = root.join("tsconfig.app.json");
    if tsconfig_app.exists() {
        resolver.load_tsconfig_paths(&tsconfig_app);
    }
    // Load workspace package name → directory mappings for monorepo support.
    resolver.load_workspace_packages();
    // Load tsconfig path aliases from all sub-project tsconfig.json files.
    // This ensures @/ aliases in workspace packages (apps/api/tsconfig.json etc.)
    // are resolved correctly, not just the root tsconfig.
    resolver.load_all_tsconfig_paths();

    let structure = Structure {
        root: root.to_path_buf(),
        entry_points,
        implicit_entries,
        source_files: source_files
            .iter()
            .map(|(rel, lang)| SourceFile {
                path: rel.clone(),
                language: lang.clone(),
            })
            .collect(),
        config_files,
    };

    let progress = crate::progress::shared_progress(source_files.len());
    progress.set_quiet(true); // TODO: wire to --quiet flag

    let (dependencies, quality, dep_graph, file_exports, file_loc, file_total_lines, file_blocks, file_sources, imported_names) =
        parse_all_files_parallel(root, &source_files, &parser, &resolver, &progress);
    progress.finish();

    let fw_profiles = crate::frameworks::detect_profiles(root);

    let issues = detect_issues(
        &all_entries,
        &structure.entry_points,
        &dep_graph,
        &file_exports,
        &file_loc,
        &file_blocks,
        &file_sources,
        root,
        &dependencies.external,
        &imported_names,
        &fw_profiles,
        &public_api,
    );

    // Use total lines (including blanks/comments) for dup % — matches jscpd/fallow methodology.
    let total_source_lines: usize = file_total_lines.values().sum();
    let duplication = crate::duplication::build_duplication_section(
        &issues.duplicate_code,
        total_source_lines,
    );

    // Detect monorepo setup.
    let monorepo = crate::monorepo::detect_monorepo(root).map(|info| MonorepoInfoData {
        kind: info.kind.to_string(),
        packages: info.packages.clone(),
    });

    // Framework names derived from profiles detected earlier.
    let detected_frameworks: Vec<String> = fw_profiles.iter().map(|p| p.name.to_string()).collect();

    Ok(AnalysisOutput {
        version: None,
        summary: None,
        detected_frameworks: Some(detected_frameworks),
        monorepo,
        structure,
        dependencies,
        quality,
        issues,
        duplication,
    })
}

/// Per-file result from parsing.
struct FileResult {
    rel_path: String,
    file_imports: FileImports,
    external_specs: Vec<String>,
    quality: FileQuality,
    loc: usize,
    total_lines: usize,
    blocks: Vec<crate::parse::blocks::CodeBlock>,
    source: String,
    dep_targets: Vec<String>,
    exports: Vec<String>,
    /// Per-resolved-target imported names: (target_file, vec_of_names).
    imported_names: Vec<(String, Vec<String>)>,
}

/// Parse all files in parallel using rayon.
fn parse_all_files_parallel(
    root: &Path,
    source_files: &[(String, String)],
    _parser: &AstParser,
    resolver: &Resolver,
    progress: &crate::progress::SharedProgress,
) -> (
    Dependencies,
    Quality,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, usize>,
    BTreeMap<String, usize>,
    BTreeMap<String, Vec<crate::parse::blocks::CodeBlock>>,
    Vec<(String, String)>,
    BTreeMap<String, HashSet<String>>,
) {
    // Wrap parser in Arc for shared access across threads.
    // AstParser uses tree-sitter which is thread-safe for parsing
    // (each parse creates its own tree).
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
        Dependencies {
            imports: all_imports,
            external: all_external.into_iter().collect(),
        },
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
fn parse_single_file(
    root: &Path,
    rel_path: &str,
    _parser: &AstParser,
    resolver: &Resolver,
) -> Option<FileResult> {
    let abs_path = root.join(rel_path);
    let source = std::fs::read_to_string(&abs_path).ok()?;
    let source_for_blocks = source.clone();

    let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
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
            Some(p) => crate::resolution::path_relative_to(root, &p),
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
    let truly_external: Vec<String> = external_specs
        .into_iter()
        .filter(|s| !resolved_external.contains(s))
        .collect();

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
        file_imports: FileImports {
            source: rel_path.to_string(),
            targets: dep_targets.clone(),
        },
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
fn resolve_file_imports_static(
    root: &Path,
    abs_path: &Path,
    specs: &[String],
    resolver: &Resolver,
) -> Vec<String> {
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



// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_nonexistent_path() {
        let result = analyze(Path::new("/no/such/path"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("path not found"));
    }
}
