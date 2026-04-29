//! Issue detection: dead code, unused exports, duplicate exports, duplicate code, gotchas,
//! unused types, circular deps, unused deps, unresolved imports, unlisted deps.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::parse::blocks::CodeBlock;
use crate::types::Issues;

pub mod circular_deps;
pub mod dead_code;
pub mod duplicate_code;
pub mod duplicate_exports;
pub mod gotchas;
pub mod unlisted_deps;
pub mod unresolved_imports;
pub mod unused_deps;
pub mod unused_exports;
pub mod unused_types;

/// Run all issue detectors.
/// * `all_entries` — union of framework + implicit entries (unused currently, kept for API compat)
/// * `framework_entries` — only framework entry points (used for dead code reachability; exports not checked for usage)
pub fn detect_issues(
    _all_entries: &[String],
    framework_entries: &[String],
    dep_graph: &BTreeMap<String, Vec<String>>,
    file_exports: &BTreeMap<String, Vec<String>>,
    file_loc: &BTreeMap<String, usize>,
    file_blocks: &BTreeMap<String, Vec<CodeBlock>>,
    file_sources: &[(String, String)],
    root: &Path,
    external_imports: &[String],
    imported_names: &BTreeMap<String, HashSet<String>>,
    profiles: &[&crate::frameworks::FrameworkProfile],
    public_api: &[String],
) -> Issues {
    Issues {
        dead_code: {
            let mut issues = dead_code::detect(_all_entries, dep_graph, file_loc);
            // Also find files only reachable through implicit entries (tooling/scripts).
            let implicit_set: Vec<String> = _all_entries
                .iter()
                .filter(|ep| !framework_entries.contains(ep))
                .cloned()
                .collect();
            issues.extend(dead_code::detect_framework_dead(
                framework_entries,
                &implicit_set,
                dep_graph,
                file_loc,
            ));
            issues
        },
        unused_exports: unused_exports::detect(
            file_exports, imported_names, framework_entries, file_sources, public_api,
        ),
        duplicate_exports: duplicate_exports::detect(file_exports),
        duplicate_code: duplicate_code::detect(file_blocks, file_sources),
        gotchas: gotchas::detect_with_frameworks(file_sources, profiles),
        unused_types: unused_types::detect(
            &extract_type_exports_map(file_sources, root),
            file_sources,
            framework_entries,
        ),
        circular_dependencies: circular_deps::detect(dep_graph),
        unused_dependencies: unused_deps::detect(root, external_imports),
        unresolved_imports: unresolved_imports::detect(dep_graph),
        unlisted_dependencies: unlisted_deps::detect(
            root,
            &build_external_import_pairs(file_sources),
        ),
    }
}

/// Build a map from file path to type exports by re-parsing each source file.
/// Includes both inline type/interface declarations AND re-exported types
/// from barrel files.
fn extract_type_exports_map(file_sources: &[(String, String)], root: &Path) -> BTreeMap<String, Vec<(String, String)>> {
    let mut map = BTreeMap::new();
    let parser = match crate::parse::AstParser::new() {
        Ok(p) => p,
        Err(_) => return map,
    };

    // Track export * from specs for second pass.
    let mut star_reexports: Vec<(String, Vec<String>)> = Vec::new();

    // First pass: extract inline types and named re-exports.
    for (path, source) in file_sources {
        let is_tsx = path.ends_with(".tsx") || path.ends_with(".jsx");
        if let Some(result) = parser.parse(source, is_tsx) {
            let root_node = result.tree.root_node();
            let mut types = crate::parse::exports::extract_type_exports(root_node, source);
            types.extend(crate::parse::exports::extract_reexport_types(root_node, source));
            if !types.is_empty() {
                map.insert(path.clone(), types);
            }

            let specs = crate::parse::exports::extract_star_reexport_specs(root_node, source);
            if !specs.is_empty() {
                star_reexports.push((path.clone(), specs));
            }
        }
    }

    // Second pass: resolve export * from targets and include their types.
    for (file_path, specs) in &star_reexports {
        let abs_dir = root.join(file_path).parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| root.to_path_buf());

        for spec in specs {
            let resolved = crate::resolution::resolve_import(&abs_dir, spec);
            if let Some(abs_target) = resolved {
                let rel_target = crate::resolution::path_relative_to(root, &abs_target);
                if let Some(target_types) = map.get(&rel_target).cloned() {
                    map.entry(file_path.clone()).or_default().extend(target_types);
                }
            }
        }
    }

    map
}

/// Build (importing_file, package_name) pairs for unlisted_deps detection.
fn build_external_import_pairs(file_sources: &[(String, String)]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let parser = match crate::parse::AstParser::new() {
        Ok(p) => p,
        Err(_) => return pairs,
    };
    for (path, source) in file_sources {
        let is_tsx = path.ends_with(".tsx") || path.ends_with(".jsx");
        if let Some(result) = parser.parse(source, is_tsx) {
            let (_internal, external) = crate::parse::imports::extract_imports(result.tree.root_node(), source);
            for pkg in external {
                pairs.push((path.clone(), pkg));
            }
        }
    }
    pairs
}
