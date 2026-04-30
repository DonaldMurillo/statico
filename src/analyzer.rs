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
    analyze_with_excludes(root, &[])
}

/// Analyze with optional exclude patterns.
pub fn analyze_with_excludes(root: &Path, exclude: &[String]) -> Result<AnalysisOutput, String> {
    if !root.exists() {
        return Err(format!("path not found: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }

    let parser = AstParser::new().map_err(|e| format!("failed to init parser: {}", e))?;

    let source_files = discover_source_files(root)?;
    let source_files = crate::discovery::filter_excluded(source_files, exclude);
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
    let is_rust = rel_path.ends_with(".rs");

    if is_rust {
        return parse_rust_file(root, rel_path, &abs_path, &source, resolver);
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

/// Parse a Rust file and return its result.
fn parse_rust_file(
    root: &Path,
    rel_path: &str,
    abs_path: &Path,
    source: &str,
    _resolver: &Resolver,
) -> Option<FileResult> {
    use crate::parse::rust::{RustAstParser, extract_exports as rust_extract_exports, extract_imports as rust_extract_imports, extract_mod_decls as rust_extract_mod_decls};

    let parser = RustAstParser::new().ok()?;
    let result = parser.parse(source)?;
    let root_node = result.tree.root_node();

    // Exports: pub fn/struct/enum/trait/const/type/mod
    let rust_exports = rust_extract_exports(source, &result.tree);
    let exports: Vec<String> = rust_exports.iter().map(|e| e.name.clone()).collect();

    // Imports: use statements → resolve to file paths
    let rust_imports = rust_extract_imports(source, &result.tree);
    let mut dep_targets: Vec<String> = Vec::new();
    let mut imported_names: Vec<(String, Vec<String>)> = Vec::new();

    for imp in &rust_imports {
        if !imp.is_glob {
            if let Some(resolved) = resolve_rust_use_path(root, rel_path, &imp.raw_path) {
                if !dep_targets.contains(&resolved) {
                    dep_targets.push(resolved.clone());
                }
                imported_names.push((resolved, imp.names.clone()));
            } else {
                imported_names.push((imp.raw_path.clone(), imp.names.clone()));
            }
        } else if let Some(resolved_glob) = resolve_rust_use_path(root, rel_path, &imp.raw_path) {
            if !dep_targets.contains(&resolved_glob) {
                dep_targets.push(resolved_glob);
            }
        }
    }

    // Module declarations: mod foo; → resolve to foo.rs or foo/mod.rs
    let mut mod_decls = rust_extract_mod_decls(source, &result.tree);

    let crate_src = find_crate_src_root(root, rel_path);
    let file_rel_to_crate = rel_path.strip_prefix(&crate_src)
        .unwrap_or(rel_path);

    // Fallback: for crate root files, discover modules by scanning the
    // filesystem. Macros like `crate_root!()` can expand to `mod de; mod ser;`
    // which tree-sitter can't see. We check for {name}.rs or {name}/mod.rs
    // files in the source directory that aren't already accounted for.
    if is_crate_root(rel_path) {
        let ts_names: std::collections::HashSet<String> = mod_decls.iter()
            .map(|m| m.name.clone()).collect();
        // Also text-scan the file for `mod <name>;` inside macro bodies
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") { continue; }
            if let Some(name) = extract_mod_name_from_line(trimmed) {
                if !ts_names.contains(&name) {
                    mod_decls.push(crate::parse::rust::RustModDecl {
                        name,
                        is_inline: false,
                        path_override: None,
                    });
                }
            }
        }
        // Also scan referenced macro definition files for mod declarations
        let updated_names: std::collections::HashSet<String> = mod_decls.iter()
            .map(|m| m.name.clone()).collect();
        // Filesystem fallback: if src/ has subdirs or .rs files not yet
        // discovered as modules, add them as likely modules
        let src_dir = root.join(&crate_src);
        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                // Check if it's a .rs file (strip extension) or a directory
                let mod_name = if name.ends_with(".rs") && name != "lib.rs" && name != "main.rs" && name != "mod.rs" {
                    name.trim_end_matches(".rs").to_string()
                } else if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    // Directory — check if it has mod.rs
                    let has_mod = root.join(format!("{}/{}/mod.rs", crate_src, name)).exists()
                        || root.join(format!("{}/{}/lib.rs", crate_src, name)).exists();
                    if has_mod { name } else { continue }
                } else {
                    continue;
                };
                if !updated_names.contains(&mod_name) {
                    mod_decls.push(crate::parse::rust::RustModDecl {
                        name: mod_name,
                        is_inline: false,
                        path_override: None,
                    });
                }
            }
        }
    }
    // For module resolution, we need the directory that `mod foo;` resolves
    // relative to. Rust rules:
    //   - src/foo/mod.rs → submodules in src/foo/
    //   - src/foo.rs → submodules in src/foo/ (the file IS the `foo` module)
    //   - src/lib.rs, src/main.rs → submodules in src/
    // current_dir_in_crate: for default mod resolution (mod foo; → foo.rs or foo/mod.rs)
    // For mod.rs/lib.rs/main.rs, submodules go in same directory.
    // For foo.rs, submodules go in foo/ directory.
    let current_dir_in_crate: String = if file_rel_to_crate.ends_with("/mod.rs")
        || file_rel_to_crate.ends_with("/lib.rs")
        || file_rel_to_crate.ends_with("/main.rs")
    {
        file_rel_to_crate.trim_start_matches('/')
            .rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string()
    } else if file_rel_to_crate.ends_with(".rs") {
        let without_ext = file_rel_to_crate.trim_end_matches(".rs");
        without_ext.trim_start_matches('/').to_string()
    } else {
        file_rel_to_crate.trim_start_matches('/')
            .rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string()
    };

    // file_dir_in_crate: actual filesystem parent directory of this file.
    // Used for #[path = "..."] resolution which is relative to the file's directory.
    let file_dir_in_crate: String = file_rel_to_crate.trim_start_matches('/')
        .rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();

    for mod_decl in &mod_decls {
        if mod_decl.is_inline {
            continue;
        }
        // If #[path = "..."] is set, use that relative to file's actual directory
        let mod_path = if let Some(ref path_ov) = mod_decl.path_override {
            let base = if file_dir_in_crate.is_empty() {
                crate_src.clone()
            } else if crate_src.is_empty() {
                file_dir_in_crate.clone()
            } else {
                format!("{}/{}", crate_src, file_dir_in_crate)
            };
            if base.is_empty() {
                path_ov.clone()
            } else {
                format!("{}/{}", base, path_ov)
            }
        } else if crate_src.is_empty() && current_dir_in_crate.is_empty() {
            mod_decl.name.clone()
        } else if crate_src.is_empty() {
            format!("{}/{}", current_dir_in_crate, mod_decl.name)
        } else if current_dir_in_crate.is_empty() {
            format!("{}/{}", crate_src, mod_decl.name)
        } else {
            format!("{}/{}/{}", crate_src, current_dir_in_crate, mod_decl.name)
        };
        if let Some(resolved) = try_rust_file(root, &mod_path) {
            if !dep_targets.contains(&resolved) {
                dep_targets.push(resolved);
            }
        }
    }

    // Metrics
    let (loc, total) = count_loc(source);
    let funcs = rust_exports.iter().filter(|e| e.kind == "function").count();
    let classes = 0;
    let cx_metrics = crate::parse::complexity::compute_metrics(root_node, source.as_bytes());
    let blocks = extract_blocks(root_node, source.as_bytes());

    Some(FileResult {
        rel_path: rel_path.to_string(),
        file_imports: FileImports {
            source: rel_path.to_string(),
            targets: dep_targets.clone(),
        },
        external_specs: vec![],
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
            parse_errors: vec![],
        },
        loc,
        total_lines: total,
        blocks,
        source: source.to_string(),
        dep_targets,
        exports,
        imported_names,
    })
}

/// Resolve a Rust `use` path to a relative file path.
///
/// Handles:
/// - `crate::foo::bar` → `src/foo/bar.rs` or `src/foo/bar/mod.rs`
/// - `super::foo` → go up from current module
/// - `self::foo` → current module's sub-module
/// - `foo::bar` → relative to current module (same as `self::foo::bar`)
///
/// Returns `None` for external crates (std, serde, etc.) and unresolvable paths.
/// Find the crate root for a given .rs file by searching upward for Cargo.toml.
/// Returns the relative path to the crate's source root directory.
/// Usually `dir/src` but can be `dir` directly when there's no `src/` subdir.
fn find_crate_src_root(root: &Path, rel_path: &str) -> String {
    // Walk up from the file's directory looking for Cargo.toml
    let mut dir = rel_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    loop {
        let cargo_path = if dir.is_empty() {
            "Cargo.toml".to_string()
        } else {
            format!("{}/Cargo.toml", dir)
        };
        if root.join(&cargo_path).exists() {
            // Found crate root. Check if src/ exists.
            let src_dir = if dir.is_empty() {
                "src".to_string()
            } else {
                format!("{}/src", dir)
            };
            if root.join(&src_dir).is_dir() {
                return src_dir;
            } else {
                // No src/ directory — source files are directly in crate root
                return dir.to_string();
            }
        }
        // Go up one level
        dir = match dir.rsplit_once('/') {
            Some((parent, _)) => parent,
            None => {
                // Reached top, check root Cargo.toml
                if root.join("Cargo.toml").exists() {
                    let src = "src";
                    if root.join(src).is_dir() {
                        return src.to_string();
                    } else {
                        return String::new(); // no src/ dir
                    }
                }
                return "src".to_string(); // fallback
            }
        };
    }
}

fn resolve_rust_use_path(root: &Path, current_rel: &str, use_path: &str) -> Option<String> {
    let crate_src = find_crate_src_root(root, current_rel);
    let parts: Vec<&str> = use_path.split("::").collect();
    if parts.is_empty() { return None; }

    // Current file's dir relative to crate src root
    let file_rel_to_crate = current_rel.strip_prefix(&crate_src)
        .unwrap_or(current_rel);
    let current_dir_in_crate = file_rel_to_crate
        .trim_start_matches('/')
        .rsplit_once('/').map(|(d, _)| d).unwrap_or("");

    match parts[0] {
        "crate" => {
            // Absolute from crate root: crate::foo::bar → {crate_src}/foo/bar
            let mod_parts = &parts[1..];
            if mod_parts.is_empty() { return None; }
            let full = format!("{}/{}", crate_src, mod_parts.join("/"));
            try_rust_file(root, &full)
        }
        "super" => {
            // Go up from current module's parent (relative to crate src)
            let mut supers = 0;
            for p in &parts {
                if *p == "super" { supers += 1; } else { break; }
            }
            let mut dir = current_dir_in_crate.to_string();
            for _ in 0..supers {
                dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();
            }
            let remaining = &parts[supers..];
            if remaining.is_empty() { return None; }
            let full = if dir.is_empty() {
                format!("{}/{}", crate_src, remaining.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, dir, remaining.join("/"))
            };
            try_rust_file(root, &full)
        }
        "self" => {
            // Current module: self::foo → current_dir/foo
            let remaining = &parts[1..];
            if remaining.is_empty() { return None; }
            let full = if current_dir_in_crate.is_empty() {
                format!("{}/{}", crate_src, remaining.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, current_dir_in_crate, remaining.join("/"))
            };
            try_rust_file(root, &full)
        }
        _ => {
            // External crate or local module name
            if is_extern_crate(parts[0]) { return None; }
            // Try as crate-internal module
            let full = if current_dir_in_crate.is_empty() {
                format!("{}/{}", crate_src, parts.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, current_dir_in_crate, parts.join("/"))
            };
            try_rust_file(root, &full)
        }
    }
}

/// Try to resolve a module path to an actual .rs file.
/// Checks: `{path}.rs`, `{path}/mod.rs`, `{path}/lib.rs`
/// Check if a file is a crate root (lib.rs, main.rs).
fn is_crate_root(rel_path: &str) -> bool {
    rel_path.ends_with("/lib.rs")
        || rel_path.ends_with("/main.rs")
        || rel_path == "lib.rs"
        || rel_path == "main.rs"
}

/// Extract `mod <name>` from a line of Rust source.
/// Handles `mod foo;`, `pub mod foo;`, `    mod foo;` etc.
/// Returns None if not a mod declaration or if it's inline (has `{`).
fn extract_mod_name_from_line(line: &str) -> Option<String> {
    // Find `mod ` in the line
    let mod_pos = line.find("mod ")?;
    let after_mod = &line[mod_pos + 4..];
    // Next should be an identifier
    let name: String = after_mod.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if name.is_empty() { return None; }
    // Check what follows the name — should be `;` (not `{`)
    let after_name = after_mod[name.len()..].trim_start();
    if after_name.starts_with(';') {
        Some(name)
    } else {
        None // inline mod or something else
    }
}

fn try_rust_file(root: &Path, module_path: &str) -> Option<String> {
    // If path already ends with .rs, check it directly
    if module_path.ends_with(".rs") {
        if root.join(module_path).exists() {
            return Some(module_path.to_string());
        }
        return None;
    }
    // 1. path.rs
    let file_path = format!("{}.rs", module_path);
    if root.join(&file_path).exists() {
        return Some(file_path);
    }
    // 2. path/mod.rs
    let mod_path = format!("{}/mod.rs", module_path);
    if root.join(&mod_path).exists() {
        return Some(mod_path);
    }
    // 3. path/lib.rs (for crate roots in workspaces)
    let lib_path = format!("{}/lib.rs", module_path);
    if root.join(&lib_path).exists() {
        return Some(lib_path);
    }
    None
}

/// Known external crate names (standard library + common).
fn is_extern_crate(name: &str) -> bool {
    matches!(name,
        "std" | "core" | "alloc" | "proc_macro" | "test"
        | "serde" | "tokio" | "hyper" | "clap" | "log" | "env_logger"
        | "regex" | "anyhow" | "thiserror" | "tracing" | "futures"
        | "async_trait" | "derive_more" | "num_traits" | "itertools"
        | "rayon" | "crossbeam" | "parking_lot" | "once_cell"
        | "lazy_static" | "indexmap" | "hashbrown" | "smallvec"
        | "bytes" | "http" | "url" | "time" | "chrono"
        | "serde_json" | "serde_derive" | "toml" | "yaml"
        | "walkdir" | "glob" | "tempfile" | "dirs"
        | "libc" | "nix" | "winapi" | "windows"
    )
}

/// Static version of resolve_file_imports_static that takes &Resolver (not &mut).
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
