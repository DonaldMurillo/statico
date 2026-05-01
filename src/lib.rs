/// Ensure a path is within a root directory (prevents path traversal attacks).
/// Canonicalizes both paths and checks that `path` starts with `root`.
pub fn ensure_within_root(path: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
    // Try canonicalize first (works for existing paths, resolves symlinks).
    // Fall back to lexical normalization for non-existent paths.
    if let (Ok(canonical), Ok(canonical_root)) =
        (std::fs::canonicalize(path), std::fs::canonicalize(root))
    {
        if !canonical.starts_with(&canonical_root) {
            return Err(format!("path '{}' escapes project root", path.display()));
        }
        return Ok(());
    }

    // Fallback: path or root doesn't exist on disk.
    // Check the relative suffix after stripping the root prefix.
    // If path starts with root, check the remaining components for `..` traversal.
    let suffix = path.strip_prefix(root).unwrap_or(path);
    let mut depth = 0i32;
    for component in suffix.components() {
        match component {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!(
                        "path '{}' escapes project root",
                        path.display()
                    ));
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::RootDir => {
                // Absolute path that isn't under root — only ok if path == root.
                if path != root {
                    return Err(format!(
                        "path '{}' is absolute outside project root",
                        path.display()
                    ));
                }
            }
            std::path::Component::CurDir => {
                // `.` doesn't change depth — no action needed.
            }
            std::path::Component::Prefix(_) => {}
        }
    }
    Ok(())
}

pub mod analyzer;
pub mod cache;
pub mod config;
pub mod discovery;
pub mod duplication;
pub mod frameworks;
pub mod issues;
pub mod languages;
pub mod monorepo;
pub mod output;
pub mod plugin;
pub mod parse;
pub mod progress;
pub mod resolution;
pub mod shell;
pub mod strip_ansi;
pub mod tui;
pub mod types;
pub mod update;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sec_path_within_root_allows_child() {
        let root = Path::new("/project");
        let child = Path::new("/project/src/index.ts");
        // These may not exist, so only the lexical path runs
        assert!(ensure_within_root(child, root).is_ok());
    }

    #[test]
    fn sec_path_within_root_rejects_parent_traversal() {
        let root = Path::new("/project");
        let evil = Path::new("/project/../../../etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject parent dir traversal");
    }

    #[test]
    fn sec_path_within_root_rejects_absolute() {
        let root = Path::new("/project");
        let evil = Path::new("/etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject absolute path outside root");
    }

    #[test]
    fn sec_path_within_root_rejects_dotdot() {
        let root = Path::new("/project");
        let evil = Path::new("/project/src/../../etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject ../ in middle");
    }

    #[test]
    fn sec_path_within_root_allows_subpath() {
        let root = Path::new("/project");
        let child = Path::new("/project/.statico/plugins/my-rule/index.ts");
        assert!(ensure_within_root(child, root).is_ok());
    }
}


