//! File discovery and entry point detection.
//!
//! Uses declarative `FrameworkProfile` definitions to detect entry points
//! and implicit entries, rather than hardcoded framework logic.

pub mod entry_points;
pub mod rust;
pub mod tooling;

use std::path::Path;

pub use entry_points::{EntryPoints, discover_entry_points};

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "rs", "py"];

/// Discover all source files in the project, returning (relative_path, language).
/// If `exclude` is provided, files matching those glob patterns are skipped.
pub fn discover_source_files(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files: Vec<(String, String)> = Vec::new();

    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| !is_skipped_dir(e.path())) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let rel = crate::resolution::path_relative_to(root, path);
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        if !SOURCE_EXTENSIONS.contains(&ext) {
            continue;
        }

        if rel.ends_with(".d.ts") {
            continue;
        }

        let lang = match ext {
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            "rs" => "rust",
            "py" => "python",
            _ => continue,
        };

        files.push((rel, lang.to_string()));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Filter source files by exclude glob patterns.
/// Returns only files that don't match any exclude pattern.
/// Supports `*` wildcard and `**` for recursive matching.
pub fn filter_excluded(files: Vec<(String, String)>, exclude: &[String]) -> Vec<(String, String)> {
    if exclude.is_empty() {
        return files;
    }
    files
        .into_iter()
        .filter(|(rel, _)| {
            for pat in exclude {
                if match_glob(pat, rel) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Simple glob matcher supporting `*` (any non-slash) and `**` (any including slashes).
fn match_glob(pattern: &str, path: &str) -> bool {
    let parts: Vec<&str> = pattern.split("**").collect();
    if parts.len() == 1 {
        // No ** — treat as simple glob
        return match_simple_glob(pattern, path);
    }
    // Split on ** and check each segment appears in order
    let mut idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = path[idx..].find(part) {
            idx += pos + part.len();
        } else if i == 0 {
            // First part must match from start
            return path.starts_with(part);
        } else {
            return false;
        }
    }
    // If pattern ends with /**, it matches everything after
    pattern.ends_with("**") || idx >= path.len()
}

/// Match a simple glob pattern (no **, just * for any non-slash chars).
fn match_simple_glob(pattern: &str, path: &str) -> bool {
    if pattern.contains('*') {
        let _regex = pattern.replace('*', "*/?"); // rough approach
        // Use a simpler approach: split on * and check segments in order
        let segments: Vec<&str> = pattern.split('*').collect();
        if segments.len() == 1 {
            return path == pattern;
        }
        let mut idx = 0;
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_empty() {
                continue;
            }
            if i == 0 {
                if !path.starts_with(seg) {
                    return false;
                }
                idx = seg.len();
            } else if i == segments.len() - 1 {
                if !path.ends_with(seg) {
                    return false;
                }
            } else {
                if let Some(pos) = path[idx..].find(seg) {
                    idx += pos + seg.len();
                } else {
                    return false;
                }
            }
        }
        return true;
    }
    path == pattern
}

/// Discover config files present in the project root.
pub fn discover_config_files(root: &Path) -> Vec<String> {
    let configs = [
        "tsconfig.json",
        "package.json",
        "jsconfig.json",
        "next.config.ts",
        "next.config.js",
        "next.config.mjs",
        "pnpm-workspace.yaml",
        "nx.json",
        "turbo.json",
        "Cargo.toml",
    ];
    let mut found: Vec<String> = Vec::new();
    for name in &configs {
        if root.join(name).exists() {
            found.push(name.to_string());
        }
    }
    found.sort();
    found
}

/// Directories to skip during file traversal.
pub fn is_skipped_dir(path: &Path) -> bool {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_entry() {
        assert_eq!(entry_points::normalize_entry("./src/index.ts"), "src/index.ts");
        assert_eq!(entry_points::normalize_entry("src/index.ts"), "src/index.ts");
        assert_eq!(entry_points::normalize_entry("./index.ts"), "index.ts");
    }

    #[test]
    fn test_is_skipped_dir() {
        assert!(is_skipped_dir(Path::new("/project/node_modules")));
        assert!(is_skipped_dir(Path::new("/project/.git")));
        assert!(is_skipped_dir(Path::new("/project/dist")));
        assert!(!is_skipped_dir(Path::new("/project/src")));
    }

    #[test]
    fn test_profiles_loaded_for_nextjs_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("nextjs-project");
        let profiles = crate::frameworks::detect_profiles(&root);
        let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
        assert!(names.contains(&"nextjs"), "expected nextjs profile, got: {:?}", names);
        assert!(names.contains(&"generic"), "expected generic fallback");
    }

    #[test]
    fn test_profiles_loaded_for_payload_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("payload-project");
        let profiles = crate::frameworks::detect_profiles(&root);
        let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
        assert!(names.contains(&"payload"), "expected payload profile, got: {:?}", names);
    }
}
