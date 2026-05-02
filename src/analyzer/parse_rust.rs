//! Rust file parsing and module resolution.

use std::path::Path;

use crate::languages::FileAnalysis;
use crate::parse::blocks::extract_blocks;
use crate::parse::metrics::count_loc;

/// Parse a Rust file and return its result as a `FileAnalysis` (plugin interface).
///
/// This is the standalone version used by the `RustPlugin` language plugin.
/// It does not need a `Resolver` because Rust has its own module resolution.
pub fn parse_rust_file_standalone(root: &Path, rel_path: &str, source: &str) -> Option<FileAnalysis> {
    parse_rust_file_inner(root, rel_path, source)
}

fn parse_rust_file_inner(root: &Path, rel_path: &str, source: &str) -> Option<FileAnalysis> {
    use crate::parse::rust::{
        RustAstParser, extract_exports as rust_extract_exports, extract_imports as rust_extract_imports,
        extract_mod_decls as rust_extract_mod_decls,
    };

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
        } else {
            // Glob import: use foo::* — marks the file as a dependency.
            // We record a wildcard marker so the unused-exports detector
            // knows ALL exports from this target are imported.
            if let Some(resolved_glob) = resolve_rust_use_path(root, rel_path, &imp.raw_path) {
                if !dep_targets.contains(&resolved_glob) {
                    dep_targets.push(resolved_glob.clone());
                }
                imported_names.push((resolved_glob, vec!["*".to_string()]));
            }
        }
    }

    // Module declarations: mod foo; → resolve to foo.rs or foo/mod.rs
    let mut mod_decls = rust_extract_mod_decls(source, &result.tree);

    let crate_src = find_crate_src_root(root, rel_path);
    let file_rel_to_crate = rel_path.strip_prefix(&crate_src).unwrap_or(rel_path);

    // Fallback: for crate root files, discover modules by scanning the
    // filesystem. Macros like `crate_root!()` can expand to `mod de; mod ser;`
    // which tree-sitter can't see. We check for {name}.rs or {name}/mod.rs
    // files in the source directory that aren't already accounted for.
    if is_crate_root(rel_path) {
        let ts_names: std::collections::HashSet<String> = mod_decls.iter().map(|m| m.name.clone()).collect();
        // Also text-scan the file for `mod <name>;` inside macro bodies
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(name) = extract_mod_name_from_line(trimmed)
                && !ts_names.contains(&name)
            {
                mod_decls.push(crate::parse::rust::RustModDecl { name, is_inline: false, path_override: None });
            }
        }
        // Also scan referenced macro definition files for mod declarations
        let updated_names: std::collections::HashSet<String> = mod_decls.iter().map(|m| m.name.clone()).collect();
        // Filesystem fallback: if src/ has subdirs or .rs files not yet
        // discovered as modules, add them as likely modules
        let src_dir = root.join(&crate_src);
        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
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
    let current_dir_in_crate: String = if file_rel_to_crate.ends_with("/mod.rs")
        || file_rel_to_crate.ends_with("/lib.rs")
        || file_rel_to_crate.ends_with("/main.rs")
    {
        file_rel_to_crate.trim_start_matches('/').rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string()
    } else if file_rel_to_crate.ends_with(".rs") {
        let without_ext = file_rel_to_crate.trim_end_matches(".rs");
        without_ext.trim_start_matches('/').to_string()
    } else {
        file_rel_to_crate.trim_start_matches('/').rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string()
    };

    // file_dir_in_crate: actual filesystem parent directory of this file.
    // Used for #[path = "..."] resolution which is relative to the file's directory.
    let file_dir_in_crate: String =
        file_rel_to_crate.trim_start_matches('/').rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();

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
            if base.is_empty() { path_ov.clone() } else { format!("{}/{}", base, path_ov) }
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
                dep_targets.push(resolved.clone());
            }
            // `mod foo;` is effectively `use foo::*` — all pub items become
            // accessible as `foo::item`. Record as a glob import.
            imported_names.push((resolved, vec!["*".to_string()]));
        }
    }

    // Metrics
    let (loc, total) = count_loc(source);
    let funcs = rust_exports.iter().filter(|e| e.kind == "function").count();
    let classes = 0;
    let cx_metrics = crate::parse::complexity::compute_metrics(root_node, source.as_bytes());
    let blocks = extract_blocks(root_node, source.as_bytes());

    Some(FileAnalysis {
        rel_path: rel_path.to_string(),
        dep_targets,
        external_specs: vec![],
        imported_names,
        exports,
        loc,
        total_lines: total,
        functions: funcs,
        classes,
        complexity: cx_metrics.complexity,
        max_nesting_depth: cx_metrics.max_nesting_depth,
        parse_errors: vec![],
        blocks,
        source: source.to_string(),
    })
}

/// Find the crate root for a given .rs file by searching upward for Cargo.toml.
/// Returns the relative path to the crate's source root directory.
/// Usually `dir/src` but can be `dir` directly when there's no `src/` subdir.
pub fn find_crate_src_root(root: &Path, rel_path: &str) -> String {
    // Walk up from the file's directory looking for Cargo.toml
    let mut dir = rel_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    loop {
        let cargo_path = if dir.is_empty() { "Cargo.toml".to_string() } else { format!("{}/Cargo.toml", dir) };
        if root.join(&cargo_path).exists() {
            // Found crate root. Check if src/ exists.
            let src_dir = if dir.is_empty() { "src".to_string() } else { format!("{}/src", dir) };
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

/// Resolve a Rust `use` path to a relative file path.
///
/// Handles:
/// - `crate::foo::bar` → `src/foo/bar.rs` or `src/foo/bar/mod.rs`
/// - `super::foo` → go up from current module
/// - `self::foo` → current module's sub-module
/// - `foo::bar` → relative to current module (same as `self::foo::bar`)
/// - `<crate_name>::foo` → same as `crate::foo` (for binary→library references)
///
/// Returns `None` for external crates (std, serde, etc.) and unresolvable paths.
/// Public wrapper for Rust use-path resolution, used by the RustPlugin.
pub fn resolve_rust_use_path_public(root: &Path, current_rel: &str, use_path: &str) -> Option<String> {
    resolve_rust_use_path(root, current_rel, use_path)
}

fn resolve_rust_use_path(root: &Path, current_rel: &str, use_path: &str) -> Option<String> {
    let crate_src = find_crate_src_root(root, current_rel);
    let parts: Vec<&str> = use_path.split("::").collect();
    if parts.is_empty() {
        return None;
    }

    // Current file's dir relative to crate src root
    let file_rel_to_crate = current_rel.strip_prefix(&crate_src).unwrap_or(current_rel);
    let current_dir_in_crate = file_rel_to_crate.trim_start_matches('/').rsplit_once('/').map(|(d, _)| d).unwrap_or("");

    // Detect if the first segment is the crate name (e.g., `statico::` in binary)
    let crate_name = detect_crate_name(root, current_rel);
    let is_crate_name = crate_name.as_ref().is_some_and(|cn| cn == parts[0]);

    match parts[0] {
        _ if parts[0] == "crate" || is_crate_name => {
            // Absolute from crate root: crate::foo::bar → {crate_src}/foo/bar
            let skip = if parts[0] == "crate" || is_crate_name { 1 } else { 0 };
            let mod_parts = &parts[skip..];
            if mod_parts.is_empty() {
                return None;
            }
            let full = format!("{}/{}", crate_src, mod_parts.join("/"));
            try_resolve_with_fallback(root, &full, mod_parts.len(), &crate_src, "")
        }
        "super" => {
            // Go up from current module's parent (relative to crate src)
            let mut supers: usize = 0;
            for p in &parts {
                if *p == "super" {
                    supers += 1;
                } else {
                    break;
                }
            }
            // For non-module-root files (e.g., src/output/ai.rs), the file itself
            // is a submodule, so the first `super` should go to the file's parent
            // directory (the module it belongs to). Additional `super`s go further up.
            let mut dir = current_dir_in_crate.to_string();
            let supers_to_apply = if !is_module_root_file(current_rel) {
                // First super: stay in current dir (go from submodule to its parent module)
                // Then go up from there for additional supers
                supers.saturating_sub(1)
            } else {
                supers
            };
            for _ in 0..supers_to_apply {
                dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or("").to_string();
            }
            let remaining = &parts[supers..];
            if remaining.is_empty() {
                return None;
            }
            let full = if dir.is_empty() {
                format!("{}/{}", crate_src, remaining.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, dir, remaining.join("/"))
            };
            try_resolve_with_fallback(root, &full, remaining.len(), &crate_src, &dir)
        }
        "self" => {
            // Current module: self::foo → current_dir/foo
            let remaining = &parts[1..];
            if remaining.is_empty() {
                return None;
            }
            let full = if current_dir_in_crate.is_empty() {
                format!("{}/{}", crate_src, remaining.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, current_dir_in_crate, remaining.join("/"))
            };
            try_resolve_with_fallback(root, &full, remaining.len(), &crate_src, current_dir_in_crate)
        }
        _ => {
            // External crate or local module name
            if is_extern_crate(parts[0]) {
                return None;
            }
            // Try as crate-internal module
            let full = if current_dir_in_crate.is_empty() {
                format!("{}/{}", crate_src, parts.join("/"))
            } else {
                format!("{}/{}/{}", crate_src, current_dir_in_crate, parts.join("/"))
            };
            try_resolve_with_fallback(root, &full, parts.len(), &crate_src, current_dir_in_crate)
        }
    }
}

/// Try resolving a module path, falling back to progressively shorter prefixes.
///
/// Rust `use` paths include the item name (e.g., `crate::issues::detect_issues`),
/// but the filesystem only has the module file (`src/issues/mod.rs`). We try:
///   1. Full path (e.g., `src/issues/detect_issues`) — might be a submodule
///   2. Strip last segment (e.g., `src/issues`) — the item is in the parent module
///   3. Continue stripping until we find a file or run out of segments
fn try_resolve_with_fallback(
    root: &Path,
    full_path: &str,
    segment_count: usize,
    _crate_src: &str,
    _dir: &str,
) -> Option<String> {
    // Try the full path first (could be a submodule like src/foo/bar.rs)
    if let Some(resolved) = try_rust_file(root, full_path) {
        return Some(resolved);
    }
    // Fallback: strip segments from the end. The last segment is likely the
    // item name (function, struct, etc.) not a module.
    // E.g., src/issues/detect_functions → src/issues (the module containing it)
    let mut current = full_path;
    for _ in 1..segment_count {
        let Some((shortened, _)) = current.rsplit_once('/') else {
            break;
        };
        if shortened.is_empty() {
            break;
        }
        if let Some(resolved) = try_rust_file(root, shortened) {
            return Some(resolved);
        }
        current = shortened;
    }
    None
}

/// Check if a file is a module root (mod.rs, lib.rs, main.rs).
/// Non-root files like `src/output/ai.rs` are submodules — `super` from them
/// should resolve to their parent directory, not go up from it.
/// Detect the crate name by reading Cargo.toml.
/// Cached per-invocation (reads once, stores in thread-local).
fn detect_crate_name(root: &Path, rel_path: &str) -> Option<String> {
    use std::cell::RefCell;
    thread_local! {
        static CACHE: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
    }
    CACHE.with(|c| {
        if c.borrow().is_some() {
            return c.borrow().as_ref().unwrap().clone();
        }
        let name = read_crate_name(root, rel_path);
        *c.borrow_mut() = Some(name.clone());
        name
    })
}

/// Read the [package].name from the Cargo.toml nearest to the file.
fn read_crate_name(root: &Path, rel_path: &str) -> Option<String> {
    let crate_src = find_crate_src_root(root, rel_path);
    // crate_src is like "src" — go up one level for Cargo.toml
    let cargo_dir = crate_src.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let cargo_path =
        if cargo_dir.is_empty() { root.join("Cargo.toml") } else { root.join(format!("{}/Cargo.toml", cargo_dir)) };
    let contents = std::fs::read_to_string(&cargo_path).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && let Some(eq) = trimmed.find('=')
        {
            let val = trimmed[eq + 1..].trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
        // Stop at the end of [package] section
        if trimmed.starts_with('[') && !trimmed.starts_with("[package") {
            break;
        }
    }
    None
}

fn is_module_root_file(rel_path: &str) -> bool {
    rel_path.ends_with("/mod.rs")
        || rel_path.ends_with("/lib.rs")
        || rel_path.ends_with("/main.rs")
        || rel_path == "lib.rs"
        || rel_path == "main.rs"
        || rel_path == "mod.rs"
}

/// Check if a file is a crate root (lib.rs, main.rs).
fn is_crate_root(rel_path: &str) -> bool {
    rel_path.ends_with("/lib.rs") || rel_path.ends_with("/main.rs") || rel_path == "lib.rs" || rel_path == "main.rs"
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
    if name.is_empty() {
        return None;
    }
    // Check what follows the name — should be `;` (not `{`)
    let after_name = after_mod[name.len()..].trim_start();
    if after_name.starts_with(';') {
        Some(name)
    } else {
        None // inline mod or something else
    }
}

/// Try to resolve a module path to an actual .rs file.
/// Checks: `{path}.rs`, `{path}/mod.rs`, `{path}/lib.rs`
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
    matches!(
        name,
        "std"
            | "core"
            | "alloc"
            | "proc_macro"
            | "test"
            | "serde"
            | "tokio"
            | "hyper"
            | "clap"
            | "log"
            | "env_logger"
            | "regex"
            | "anyhow"
            | "thiserror"
            | "tracing"
            | "futures"
            | "async_trait"
            | "derive_more"
            | "num_traits"
            | "itertools"
            | "rayon"
            | "crossbeam"
            | "parking_lot"
            | "once_cell"
            | "lazy_static"
            | "indexmap"
            | "hashbrown"
            | "smallvec"
            | "bytes"
            | "http"
            | "url"
            | "time"
            | "chrono"
            | "serde_json"
            | "serde_derive"
            | "toml"
            | "yaml"
            | "walkdir"
            | "glob"
            | "tempfile"
            | "dirs"
            | "libc"
            | "nix"
            | "winapi"
            | "windows"
    )
}
