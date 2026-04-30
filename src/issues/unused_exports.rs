//! Unused export detection — individual named exports that no file imports.
//! Excludes exports from:
//!   1. Entry point files (frameworks consume those by convention)
//!   2. Barrel re-export files (index.ts/js that re-export from sub-modules)
//!      These are the public API surface of packages — their exports are
//!      consumed externally even if no internal file imports them.

use std::collections::{BTreeMap, HashSet};

use crate::types::UnusedExportIssue;

/// Find named exports that no other file imports.
/// `file_exports` maps file path → its export names.
/// `imported_names` maps target file path → names imported FROM that file (by any importer).
/// Files that are entry points are excluded — the framework consumes their exports.
/// Barrel re-export files (index.ts/js that primarily re-export) are also excluded —
/// their exports form the package's public API.
pub fn detect(
    file_exports: &BTreeMap<String, Vec<String>>,
    imported_names: &BTreeMap<String, HashSet<String>>,
    entry_points: &[String],
    file_sources: &[(String, String)],
    public_api: &[String],
    root: &std::path::Path,
) -> Vec<UnusedExportIssue> {
    let ep_set: HashSet<&str> = entry_points.iter().map(|s| s.as_str()).collect();
    let pa_set: HashSet<&str> = public_api.iter().map(|s| s.as_str()).collect();

    // Build a set of barrel re-export files — index.ts/js files whose content
    // is primarily re-exports. These form the package's public API surface.
    let barrel_files = detect_barrel_files(file_sources);

    let _root = root; // Used by deep-resolution feature
    let mut unused: Vec<UnusedExportIssue> = Vec::new();
    for (path, exports) in file_exports {
        if ep_set.contains(path.as_str()) {
            continue;
        }

        // Skip public API files — their exports are the package's external interface.
        if pa_set.contains(path.as_str()) {
            continue;
        }

        // Skip barrel re-export files — their exports are the package's public API.
        if barrel_files.contains(path.as_str()) {
            continue;
        }

        // Skip golden/snapshot/fixture test files — they're expected outputs, not source.
        if is_test_fixture(path) {
            continue;
        }

        // Skip example/demo/sample/integration directories — their exports
        // showcase library usage and aren't consumed by the main codebase.
        if is_example_directory(path) {
            continue;
        }

        // Rust: lib.rs is the crate root — its re-exports are the public API.
        if is_rust_lib_file(path) {
            continue;
        }

        let names_imported = imported_names.get(path);

        // Check for Rust glob import: if any file does `use foo::*`,
        // all exports from `foo` are considered used.
        let has_glob_import = names_imported.is_some_and(|set| set.contains("*"));

        for name in exports {
            if has_glob_import {
                continue;
            }
            let is_used = names_imported.is_some_and(|set| set.contains(name));
            if !is_used {
                unused.push(UnusedExportIssue { name: name.clone(), path: path.clone() });
            }
        }
    }

    // Deep resolution: trace re-export chains.
    // When barrel file B re-exports `Foo` from source file A, and some consumer
    // imports `Foo` from barrel B, then A's `Foo` is transitively used.
    // Also handles multi-level chains: C→B→A via BFS.
    #[cfg(feature = "deep-resolution")]
    {
        let reexport_map = build_reexport_map(file_sources, imported_names, _root);
        unused.retain(|issue| {
            // BFS: start from (issue.path, issue.name), follow re-export chains upward.
            // A file's export is "transitively used" if any file in the chain
            // has the name directly imported.
            let mut visited: HashSet<(String, String)> = HashSet::new();
            let mut queue: std::collections::VecDeque<(String, String)> = std::collections::VecDeque::new();
            queue.push_back((issue.path.clone(), issue.name.clone()));

            while let Some((file, name)) = queue.pop_front() {
                // Check: is this (file, name) directly imported somewhere?
                if let Some(names) = imported_names.get(file.as_str()) {
                    if names.contains(&name) {
                        return false; // Transitively used
                    }
                }

                // Follow re-export chains: who re-exports `name` from `file`?
                // Named: (file, name) → barrel files
                if let Some(barrels) = reexport_map.get(&(file.clone(), name.clone())) {
                    for barrel in barrels {
                        let key = (barrel.clone(), name.clone());
                        if visited.insert(key.clone()) {
                            queue.push_back(key);
                        }
                    }
                }
                // Star: (file, "*") → barrel files that re-export everything
                if let Some(barrels) = reexport_map.get(&(file.clone(), "*".to_string())) {
                    for barrel in barrels {
                        let key = (barrel.clone(), name.clone());
                        if visited.insert(key.clone()) {
                            queue.push_back(key);
                        }
                    }
                }
            }
            true // Not transitively used
        });
    }

    unused.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
    unused
}

/// Detect barrel re-export files — files whose primary purpose is to re-export
/// from sub-modules. These form a package's public API surface.
///
/// A file is considered a barrel if:
///   - It's named index.ts/tsx/js/jsx AND >60% of statements are re-exports, OR
///   - ANY filename where >80% of statements are re-exports
///     (catches files like charts.tsx, core_private_export.ts)
///
/// Re-exports are counted by STATEMENTS (including multi-line `export { ... } from '...'`),
/// not by individual lines. A multi-line re-export block counts as one statement.
fn detect_barrel_files(file_sources: &[(String, String)]) -> HashSet<String> {
    let mut barrels = HashSet::new();
    for (path, source) in file_sources {
        let filename = path.rsplit('/').next().unwrap_or(path);
        let is_index =
            filename == "index.ts" || filename == "index.tsx" || filename == "index.js" || filename == "index.jsx";

        let mut total_stmts = 0usize;
        let mut reexport_stmts = 0usize;
        // Track whether we're inside a multi-line export { ... } from block.
        let mut in_export_block = false;
        let mut export_block_has_from = false;

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*")
            {
                continue;
            }

            if in_export_block {
                // Check if this line closes the block.
                if trimmed.starts_with('}') || trimmed.contains("} from ") || trimmed.contains("}from ") {
                    if trimmed.contains(" from ") || trimmed.contains("from '") || trimmed.contains("from \"") {
                        export_block_has_from = true;
                    }
                    // Block closed if: has semicolon, ends with }, or is a "} from '...'" re-export
                    let block_closed = trimmed.contains(';') || trimmed.ends_with('}') || trimmed.contains("} from ");
                    if block_closed {
                        total_stmts += 1;
                        if export_block_has_from {
                            reexport_stmts += 1;
                        }
                        in_export_block = false;
                        export_block_has_from = false;
                    }
                }
                continue;
            }

            total_stmts += 1;

            // Single-line re-export: export { ... } from '...'
            if trimmed.starts_with("export {") && (trimmed.contains("} from ") || trimmed.ends_with('}')) {
                if trimmed.contains(" from ") {
                    reexport_stmts += 1;
                } else if trimmed.ends_with('}') {
                    // Multi-line export block starting on this line
                    in_export_block = true;
                    export_block_has_from = false;
                    total_stmts -= 1; // Will be counted when block closes
                }
                continue;
            }
            // Multi-line export block: line is just "export {"
            if trimmed == "export {" {
                in_export_block = true;
                export_block_has_from = false;
                total_stmts -= 1; // Will be counted when block closes
                continue;
            }
            // export * from '...'
            if trimmed.starts_with("export *") {
                reexport_stmts += 1;
                continue;
            }
            // export type { ... } from '...'
            if trimmed.starts_with("export type {") {
                if trimmed.contains(" from ") && trimmed.contains(';') {
                    reexport_stmts += 1;
                } else {
                    in_export_block = true;
                    export_block_has_from = false;
                    total_stmts -= 1;
                }
                continue;
            }
            // export { ... } spanning multiple lines without opening on first line
            if trimmed.starts_with("export") && trimmed.contains(" from ") {
                reexport_stmts += 1;
                continue;
            }
        }

        if total_stmts == 0 {
            continue;
        }
        let ratio = reexport_stmts as f64 / total_stmts as f64;
        // A file is a barrel if:
        //   - It's 100% re-exports with at least 1 re-export (catches icon wrappers), OR
        //   - It's an index file with >60% re-exports, OR
        //   - It's a non-index file with >30% re-exports AND ≥3 re-export stmts
        let is_barrel =
            (ratio >= 1.0 && reexport_stmts >= 1) || (is_index && ratio > 0.6) || (!is_index && ratio > 0.3 && reexport_stmts >= 3);
        if is_barrel {
            barrels.insert(path.clone());
        }
    }
    barrels
}

/// Check if a file is a test fixture (golden/snapshot/expected output)
/// that shouldn't be analyzed for unused exports.
fn is_example_directory(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.starts_with("integration/")
        || lower.starts_with("sample/")
        || lower.starts_with("samples/")
        || lower.starts_with("examples/")
        || lower.starts_with("example/")
        || lower.starts_with("demo/")
        || lower.starts_with("demos/")
        || lower.contains("/integration/")
        || lower.contains("/sample/")
        || lower.contains("/samples/")
        || lower.contains("/examples/")
        || lower.contains("/example/")
        || lower.contains("/demo/")
        || lower.contains("/demos/")
}

/// Build a reverse re-export map: (source_file, export_name) → set of barrel files
/// that re-export that name from that source.
/// This enables transitive "used" detection: if barrel B re-exports `Foo` from A,
/// and someone imports `Foo` from B, then A's `Foo` is transitively used.
#[cfg(feature = "deep-resolution")]
fn build_reexport_map(
    file_sources: &[(String, String)],
    _imported_names: &BTreeMap<String, HashSet<String>>,
    root: &std::path::Path,
) -> BTreeMap<(String, String), Vec<String>> {
    use crate::parse::AstParser;

    let mut map: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let parser = match AstParser::new() {
        Ok(p) => p,
        Err(_) => return map,
    };

    for (path, source) in file_sources {
        // Skip files that clearly have no re-exports (cheap string pre-filter).
        // We check for "export *" and "export {" + "from" patterns.
        let has_star_reexport = source.contains("export *") || source.contains("export *");
        // Named re-exports: "export { ... } from" — check for both patterns
        let has_named_reexport = source.contains("} from") && source.contains("export {");
        if !has_star_reexport && !has_named_reexport {
            continue;
        }

        let is_tsx = path.ends_with(".tsx");
        let parsed = match parser.parse(source, is_tsx) {
            Some(p) => p,
            None => continue,
        };

        // The barrel's directory as an absolute path for resolution.
        let barrel_dir_str = path.rsplit_once('/').map(|p| p.0).unwrap_or(".");
        let barrel_dir_abs = root.join(barrel_dir_str);

        // Extract named re-exports: export { Foo, Bar } from './baz'
        let reexports = crate::parse::exports::extract_reexport_sources(parsed.tree.root_node(), source);
        for (name, rel_spec) in &reexports {
            let resolved = crate::resolution::resolve_import(&barrel_dir_abs, rel_spec);
            if let Some(target_abs) = resolved {
                let target_rel = crate::resolution::path_relative_to(root, &target_abs);
                map.entry((target_rel, name.clone())).or_default().push(path.clone());
            }
        }

        // Also handle star re-exports: export * from './foo'
        let star_specs = crate::parse::exports::extract_star_reexport_specs(parsed.tree.root_node(), source);
        for rel_spec in &star_specs {
            let resolved = crate::resolution::resolve_import(&barrel_dir_abs, rel_spec);
            if let Some(target_abs) = resolved {
                let target_rel = crate::resolution::path_relative_to(root, &target_abs);
                map.entry((target_rel, "*".to_string())).or_default().push(path.clone());
            }
        }
    }

    map
}

fn is_test_fixture(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("golden")
        || lower.contains(".snap.")
        || lower.contains(".expected.")
        || lower.contains(".baseline.")
        || lower.contains("__snapshots__")
        || lower.contains("__fixtures__")
        || lower.contains("__mocks__")
        || lower.ends_with(".snap")
        // Test files — their exports are for test runners, not production code.
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/__tests__/")
        || lower.contains("/__test__/")
        || lower.contains("/__mocks__/")
        // Config files — consumed by tooling, not imported.
        || lower.ends_with(".config.ts")
        || lower.ends_with(".config.js")
        || lower.ends_with(".config.mjs")
        // Auto-generated type files (e.g. payload-types.ts, payload-generated-schema.ts)
        || lower.ends_with("-types.ts")
        || lower.ends_with("-generated-schema.ts")
        || lower.ends_with("-generated-types.ts")
        // Rust test files
        || lower.ends_with("_test.rs")
        || lower.contains("/tests/")
        || lower.contains("/examples/")
        || lower.ends_with("/benches/")
}

/// Check if a file is a Rust lib.rs — the crate root whose re-exports
/// form the public API surface (similar to barrel/index files in TS).
fn is_rust_lib_file(path: &str) -> bool {
    path.ends_with("/lib.rs") || path == "lib.rs"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_when_file_not_imported() {
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["helper".into(), "formatDate".into()]),
            ("src/index.ts".into(), vec!["main".into()]),
        ]);
        // Nothing imports from utils.ts
        let imported = BTreeMap::new();
        let eps = vec!["src/index.ts".into()];
        let sources = vec![];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        // utils.ts is not an entry point and nothing imports its names.
        assert_eq!(unused.len(), 2);
        assert_eq!(unused[0].name, "formatDate");
        assert_eq!(unused[1].name, "helper");
    }

    #[test]
    fn flagged_when_not_entry_point_and_not_imported() {
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["helper".into()]),
            ("src/dead.ts".into(), vec!["oldThing".into()]),
        ]);
        let imported = BTreeMap::from([("src/utils.ts".into(), HashSet::from(["helper".into()]))]);
        let eps = vec!["src/app.ts".into()];
        let sources = vec![];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "oldThing");
        assert_eq!(unused[0].path, "src/dead.ts");
    }

    #[test]
    fn all_used_when_every_name_is_imported() {
        let exports = BTreeMap::from([("src/utils.ts".into(), vec!["helper".into()])]);
        let imported = BTreeMap::from([("src/utils.ts".into(), HashSet::from(["helper".into()]))]);
        let sources = vec![];
        let unused = detect(&exports, &imported, &[], &sources, &[], std::path::Path::new("."));
        assert!(unused.is_empty());
    }

    #[test]
    fn detects_partial_unused() {
        // File exports foo, bar, baz. Only foo is imported → bar and baz are unused.
        let exports = BTreeMap::from([("src/utils.ts".into(), vec!["foo".into(), "bar".into(), "baz".into()])]);
        let imported = BTreeMap::from([("src/utils.ts".into(), HashSet::from(["foo".into()]))]);
        let sources = vec![];
        let unused = detect(&exports, &imported, &[], &sources, &[], std::path::Path::new("."));
        assert_eq!(unused.len(), 2);
        assert_eq!(unused[0].name, "bar");
        assert_eq!(unused[1].name, "baz");
    }

    #[test]
    fn entry_point_exports_excluded() {
        let exports = BTreeMap::from([
            ("src/index.ts".into(), vec!["main".into(), "App".into()]),
            ("src/utils.ts".into(), vec!["helper".into()]),
        ]);
        // Nobody imports anything from either file.
        let imported = BTreeMap::new();
        let eps = vec!["src/index.ts".into()];
        let sources = vec![];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        // Only utils.ts exports should be flagged (index.ts is an entry point).
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "helper");
        assert_eq!(unused[0].path, "src/utils.ts");
    }

    #[test]
    fn barrel_reexport_file_excluded() {
        // A barrel index.ts that re-exports everything — should be treated as public API.
        let barrel_source =
            "export { Button } from './button'\nexport { Input } from './input'\nexport { Select } from './select'\n";
        let exports = BTreeMap::from([("src/index.ts".into(), vec!["Button".into(), "Input".into(), "Select".into()])]);
        let imported = BTreeMap::new(); // nobody imports these internally
        let eps = vec![]; // not an explicit entry point
        let sources = vec![("src/index.ts".to_string(), barrel_source.to_string())];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        // Barrel file exports should be excluded — they're the public API.
        assert!(unused.is_empty(), "barrel re-export file should not have unused exports");
    }

    #[test]
    fn non_barrel_index_still_flagged() {
        // An index.ts that has actual code, not just re-exports.
        let index_source = "export function helper() { return 42; }\nexport function unusedHelper() { return 0; }\n";
        let exports = BTreeMap::from([("src/index.ts".into(), vec!["helper".into(), "unusedHelper".into()])]);
        let imported = BTreeMap::from([("src/index.ts".into(), HashSet::from(["helper".into()]))]);
        let eps = vec![];
        let sources = vec![("src/index.ts".to_string(), index_source.to_string())];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        // Not a barrel file — unusedHelper should still be flagged.
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "unusedHelper");
    }

    #[test]
    fn non_index_barrel_file_excluded() {
        // A non-index file (charts.tsx) that is primarily re-exports.
        let barrel_source = "export { ChartArea } from './chart-area'\nexport { ChartBar } from './chart-bar'\nexport { ChartLine } from './chart-line'\nexport { ChartPie } from './chart-pie'\n";
        let exports = BTreeMap::from([(
            "app/charts/charts.tsx".into(),
            vec!["ChartArea".into(), "ChartBar".into(), "ChartLine".into(), "ChartPie".into()],
        )]);
        let imported = BTreeMap::new();
        let eps = vec![];
        let sources = vec![("app/charts/charts.tsx".to_string(), barrel_source.to_string())];
        let unused = detect(&exports, &imported, &eps, &sources, &[], std::path::Path::new("."));
        // Non-index barrel file should also be excluded.
        assert!(unused.is_empty(), "non-index barrel re-export file should not have unused exports");
    }

    #[test]
    fn public_api_file_excluded() {
        // A file that's declared as a package's public API via package.json.
        let exports = BTreeMap::from([(
            "packages/ui/src/icons.tsx".into(),
            vec!["IconA".into(), "IconB".into(), "IconC".into()],
        )]);
        let imported = BTreeMap::new(); // nobody imports these internally
        let eps = vec![];
        let sources = vec![];
        let public_api = vec!["packages/ui/src/icons.tsx".to_string()];
        let unused = detect(&exports, &imported, &eps, &sources, &public_api, std::path::Path::new("."));
        // Public API file exports should be excluded.
        assert!(unused.is_empty(), "public API file should not have unused exports");
    }
}
