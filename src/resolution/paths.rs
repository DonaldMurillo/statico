//! Path resolution helpers.
//!
//! Handles relative import resolution, extension trying, and canonicalization.

use std::path::{Path, PathBuf};

pub(crate) const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "rs"];

/// Resolve a relative import specifier to an actual file path.
pub(super) fn resolve_relative(from_dir: &Path, spec: &str) -> Option<PathBuf> {
    let candidate = from_dir.join(spec);
    try_extensions(&candidate)
}

/// Try to find a file at the given path, with various extensions.
pub(crate) fn try_extensions(candidate: &Path) -> Option<PathBuf> {
    // Try exact path.
    if candidate.is_file() {
        return Some(canonicalize(candidate));
    }

    // Try appending extensions (e.g. "app.component" + ".ts" = "app.component.ts").
    // Must try append BEFORE with_extension, because with_extension replaces
    // the existing extension: "app.component".with_extension("ts") → "app.ts"
    // which is wrong for Angular/NestJS naming conventions.
    let candidate_str = candidate.to_string_lossy();
    for ext in SOURCE_EXTENSIONS {
        let appended = format!("{}.{}", candidate_str, ext);
        let appended_path = Path::new(&appended);
        if appended_path.is_file() {
            return Some(canonicalize(appended_path));
        }
    }

    // Also try with_extension (replaces extension) for cases like "foo.js" → "foo.ts".
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

pub(crate) fn canonicalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

