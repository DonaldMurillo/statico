use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::parse::{count_classes, count_functions, compute_complexity, count_loc, AstParser};
use crate::types::*;

/// Run the full analysis on a project directory.
pub fn analyze(root: &Path) -> Result<AnalysisOutput, String> {
    if !root.exists() {
        return Err(format!("path not found: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }

    let mut parser = AstParser::new().map_err(|e| format!("failed to init parser: {}", e))?;

    let source_files = discover_source_files(root)?;
    let config_files = discover_config_files(root)?;
    let entry_points = discover_entry_points(root, &source_files)?;

    let structure = Structure {
        root: root.to_path_buf(),
        entry_points,
        source_files: source_files
            .iter()
            .map(|(rel, lang)| SourceFile {
                path: rel.clone(),
                language: lang.clone(),
            })
            .collect(),
        config_files,
    };

    // Parse all files and extract imports + metrics.
    let mut all_imports: Vec<FileImports> = Vec::new();
    let mut all_external: BTreeSet<String> = BTreeSet::new();
    let mut quality_files: Vec<FileQuality> = Vec::new();

    for (rel_path, _lang) in &source_files {
        let abs_path = root.join(rel_path);
        let source = match std::fs::read_to_string(&abs_path) {
            Ok(s) => s,
            Err(e) => {
                quality_files.push(FileQuality {
                    path: rel_path.clone(),
                    metrics: None,
                    parse_errors: vec![ParseError {
                        message: format!("failed to read file: {}", e),
                        line: 0,
                        column: 0,
                    }],
                });
                all_imports.push(FileImports {
                    source: rel_path.clone(),
                    targets: vec![],
                });
                continue;
            }
        };

        let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
        let result = match parser.parse(&source, is_tsx) {
            Some(r) => r,
            None => {
                quality_files.push(FileQuality {
                    path: rel_path.clone(),
                    metrics: None,
                    parse_errors: vec![ParseError {
                        message: "parse returned None".to_string(),
                        line: 0,
                        column: 0,
                    }],
                });
                all_imports.push(FileImports {
                    source: rel_path.clone(),
                    targets: vec![],
                });
                continue;
            }
        };

        let root_node = result.tree.root_node();

        // Extract imports.
        let (internal_specs, external_specs) =
            crate::parse::extract_imports(root_node, &source);

        // Resolve internal specifiers to actual file paths.
        let file_dir = abs_path.parent().unwrap_or(root);
        let mut resolved_targets: Vec<String> = Vec::new();
        for spec in &internal_specs {
            if let Some(resolved) = resolve_import(file_dir, root, spec) {
                let rel = path_relative_to(root, &resolved);
                if !resolved_targets.contains(&rel) {
                    resolved_targets.push(rel);
                }
            } else {
                // Keep the unresolved specifier as-is.
                if !resolved_targets.contains(spec) {
                    resolved_targets.push(spec.clone());
                }
            }
        }
        resolved_targets.sort();

        for ext in &external_specs {
            all_external.insert(ext.clone());
        }

        all_imports.push(FileImports {
            source: rel_path.clone(),
            targets: resolved_targets,
        });

        // Extract quality metrics.
        if result.has_errors {
            let errors = crate::parse::collect_errors(root_node, source.as_bytes());
            let parse_errors: Vec<ParseError> = errors
                .into_iter()
                .map(|(msg, line, col)| ParseError {
                    message: msg,
                    line,
                    column: col,
                })
                .collect();

            // Still compute metrics if we got a partial tree.
            let (loc, total) = count_loc(&source);
            let funcs = count_functions(root_node);
            let classes = count_classes(root_node);
            let complexity = compute_complexity(root_node, &source);

            quality_files.push(FileQuality {
                path: rel_path.clone(),
                metrics: Some(Metrics {
                    lines_of_code: loc,
                    total_lines: total,
                    functions: funcs,
                    classes,
                    complexity,
                }),
                parse_errors,
            });
        } else {
            let (loc, total) = count_loc(&source);
            let funcs = count_functions(root_node);
            let classes = count_classes(root_node);
            let complexity = compute_complexity(root_node, &source);

            quality_files.push(FileQuality {
                path: rel_path.clone(),
                metrics: Some(Metrics {
                    lines_of_code: loc,
                    total_lines: total,
                    functions: funcs,
                    classes,
                    complexity,
                }),
                parse_errors: vec![],
            });
        }
    }

    // Sort for determinism.
    all_imports.sort_by(|a, b| a.source.cmp(&b.source));
    quality_files.sort_by(|a, b| a.path.cmp(&b.path));

    let dependencies = Dependencies {
        imports: all_imports,
        external: all_external.into_iter().collect(),
    };

    let quality = Quality {
        files: quality_files,
    };

    Ok(AnalysisOutput {
        structure,
        dependencies,
        quality,
    })
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

fn discover_source_files(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files: Vec<(String, String)> = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skipped_dir(e.path(), root))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let rel = path_relative_to(root, path);
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        if !SOURCE_EXTENSIONS.contains(&ext) {
            continue;
        }

        // Skip .d.ts declaration files.
        if rel.ends_with(".d.ts") {
            continue;
        }

        let lang = match ext {
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            _ => continue,
        };

        files.push((rel, lang.to_string()));
    }

    // Sort for determinism.
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn discover_config_files(root: &Path) -> Result<Vec<String>, String> {
    let configs = ["tsconfig.json", "package.json", "jsconfig.json"];
    let mut found: Vec<String> = Vec::new();
    for name in &configs {
        if root.join(name).exists() {
            found.push(name.to_string());
        }
    }
    found.sort();
    Ok(found)
}

fn discover_entry_points(
    root: &Path,
    source_files: &[(String, String)],
) -> Result<Vec<String>, String> {
    let mut entry_points: BTreeSet<String> = BTreeSet::new();

    // Check package.json for main/module/exports fields.
    if let Ok(content) = std::fs::read_to_string(root.join("package.json"))
        && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
    {
        for field in &["main", "module", "browser"] {
            if let Some(val) = pkg.get(field).and_then(|v| v.as_str()) {
                let rel = normalize_entry(val);
                if source_files.iter().any(|(p, _)| p == &rel) {
                    entry_points.insert(rel);
                } else {
                    // The entry might reference a .js file that has a .ts counterpart.
                    let ts_rel = rel
                        .replace(".js", ".ts")
                        .replace(".jsx", ".tsx");
                    if source_files.iter().any(|(p, _)| p == &ts_rel) {
                        entry_points.insert(ts_rel);
                    } else {
                        entry_points.insert(rel);
                    }
                }
            }
        }
        // Check exports field.
        if let Some(exports) = pkg.get("exports") {
            extract_exports_paths(exports, &mut entry_points);
        }
    }

    // Check tsconfig.json for files/include fields.
    if let Ok(content) = std::fs::read_to_string(root.join("tsconfig.json"))
        && let Ok(tsconfig) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(files) = tsconfig.get("files").and_then(|v| v.as_array())
    {
        for f in files {
            if let Some(s) = f.as_str() {
                entry_points.insert(s.to_string());
            }
        }
    }

    // Default entry point locations.
    let defaults = [
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "src/index.jsx",
        "src/main.ts",
        "src/main.tsx",
        "index.ts",
        "index.tsx",
        "index.js",
        "index.jsx",
        "main.ts",
    ];
    for def in &defaults {
        if source_files.iter().any(|(p, _)| p == *def) {
            entry_points.insert(def.to_string());
        }
    }

    let mut result: Vec<String> = entry_points.into_iter().collect();
    result.sort();
    Ok(result)
}

fn extract_exports_paths(exports: &serde_json::Value, paths: &mut BTreeSet<String>) {
    match exports {
        serde_json::Value::String(s) => {
            paths.insert(s.clone());
        }
        serde_json::Value::Object(map) => {
            // Look for "." key or iterate all keys.
            for val in map.values() {
                extract_exports_paths(val, paths);
            }
        }
        _ => {}
    }
}

fn normalize_entry(path: &str) -> String {
    let p = path.trim_start_matches("./");
    p.to_string()
}

// ---------------------------------------------------------------------------
// Import resolution
// ---------------------------------------------------------------------------

/// Try to resolve a relative import specifier to an actual file path.
fn resolve_import(from_dir: &Path, _project_root: &Path, spec: &str) -> Option<PathBuf> {
    let candidate = from_dir.join(spec);

    // Try exact path.
    if candidate.is_file() {
        return Some(canonicalize(&candidate));
    }

    // Try with extensions.
    for ext in SOURCE_EXTENSIONS {
        let with_ext = candidate.with_extension(ext);
        if with_ext.is_file() {
            return Some(canonicalize(&with_ext));
        }
    }

    // Try index file in directory.
    if candidate.is_dir() {
        for ext in SOURCE_EXTENSIONS {
            let index = candidate.join(format!("index.{}", ext));
            if index.is_file() {
                return Some(canonicalize(&index));
            }
        }
    }

    None
}

fn canonicalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Get a path relative to the root.
fn path_relative_to(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
    .replace('\\', "/")
}

/// Directories to skip during file traversal.
fn is_skipped_dir(path: &Path, _root: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(
        name,
        "node_modules"
            | ".git"
            | "dist"
            | "build"
            | "out"
            | ".next"
            | ".nuxt"
            | "coverage"
            | ".cache"
            | "target"
            | ".turbo"
    )
}
