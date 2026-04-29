//! Import resolution and path helpers.
//!
//! Handles:
//!   - Relative imports (`./foo`, `../bar`)
//!   - tsconfig `paths` aliases (`@/components/foo` → `./src/components/foo`)
//!   - Extension resolution (try `.ts`, `.tsx`, `.js`, `.jsx`, `index.ts`, etc.)

use std::path::{Path, PathBuf};

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

/// A tsconfig `paths` alias mapping.
/// E.g. `@/*` → `["./src/*"]` becomes `PathAlias { prefix: "@/", targets: ["./src/"] }`.
#[derive(Debug, Clone)]
pub struct PathAlias {
    /// The alias prefix (e.g. `@/`). Always ends with `/` or is exact (no wildcard).
    pub prefix: String,
    /// The target directories to try (e.g. `["./src/"]`).
    pub targets: Vec<String>,
}

/// Resolver that handles tsconfig path aliases.
#[derive(Clone)]
pub struct Resolver {
    aliases: Vec<PathAlias>,
    root: PathBuf,
}

impl Resolver {
    /// Create a new resolver for the given project root.
    pub fn new(root: &Path) -> Self {
        Self {
            aliases: Vec::new(),
            root: root.to_path_buf(),
        }
    }

    /// Load path aliases from a tsconfig.json file (if it exists).
    pub fn load_tsconfig_paths(&mut self, tsconfig_path: &Path) {
        let content = match std::fs::read_to_string(tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let tsconfig: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        let paths = match tsconfig
            .get("compilerOptions")
            .and_then(|co| co.get("paths"))
            .and_then(|p| p.as_object())
        {
            Some(p) => p,
            None => return,
        };

        for (pattern, targets) in paths {
            let is_wildcard = pattern.ends_with('*');
            let prefix = if is_wildcard {
                // `@/*` → `@/`
                pattern.trim_end_matches('*').to_string()
            } else {
                pattern.clone()
            };

            let target_list: Vec<String> = targets
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .map(|t| {
                            if t.ends_with('*') {
                                t.trim_end_matches('*').to_string()
                            } else {
                                t.clone()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            if !prefix.is_empty() && !target_list.is_empty() {
                self.aliases.push(PathAlias {
                    prefix,
                    targets: target_list,
                });
            }
        }
    }

    /// Get the loaded aliases (for testing/debugging).
    pub fn aliases(&self) -> &[PathAlias] {
        &self.aliases
    }

    /// Load workspace package mappings from all package.json files under the root.
    /// Maps `@scope/name` → `<root>/<pkg_dir>/src/index.ts` (or whatever `main` points to).
    pub fn load_workspace_packages(&mut self) {
        // Walk all package.json files under the root.
        for entry in walkdir::WalkDir::new(&*self.root)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() || path.file_name().map_or(true, |n| n != "package.json") {
                continue;
            }
            let rel = path_relative_to(&self.root, path);
            // Skip node_modules package.json files.
            if rel.contains("node_modules") {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let pkg: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let name = match pkg.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };
            // Only map scoped or namespaced packages (starts with @ or contains /).
            if !name.starts_with('@') && !name.contains('/') {
                continue;
            }

            let main = pkg
                .get("main")
                .and_then(|v| v.as_str())
                .unwrap_or("src/index.ts");

            // Package dir is the parent of package.json.
            let pkg_dir = match path.parent() {
                Some(d) => d,
                None => continue,
            };

            let target = pkg_dir.join(main);
            let target_rel = path_relative_to(&self.root, &target);

            self.aliases.push(PathAlias {
                prefix: format!("{}/", name),
                targets: vec![format!("./{}/", pkg_dir.join(main).parent().map(|p| path_relative_to(&self.root, p)).unwrap_or_default())],
            });

            // Also register the bare package name pointing to main.
            self.aliases.push(PathAlias {
                prefix: name.to_string(),
                targets: vec![format!("./{}", target_rel)],
            });
        }
    }

    /// Resolve an import specifier relative to the given file directory.
    /// Returns the resolved absolute path, or None if unresolvable.
    pub fn resolve(&self, from_dir: &Path, spec: &str) -> Option<PathBuf> {
        // 1. Try relative imports first.
        if spec.starts_with('.') || spec.starts_with('/') {
            return resolve_relative(from_dir, spec);
        }

        // 2. Try tsconfig path aliases.
        for alias in &self.aliases {
            if let Some(rest) = spec.strip_prefix(&alias.prefix) {
                for target in &alias.targets {
                    let resolved_path = self.root.join(target).join(rest);
                    if let Some(found) = try_extensions(&resolved_path) {
                        return Some(found);
                    }
                }
            }
        }

        // 3. Try as relative to root (for bare specifiers like "src/foo").
        let from_root = self.root.join(spec);
        if let Some(found) = try_extensions(&from_root) {
            return Some(found);
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Path resolution helpers
// ---------------------------------------------------------------------------

/// Resolve a relative import specifier to an actual file path.
pub fn resolve_import(from_dir: &Path, spec: &str) -> Option<PathBuf> {
    resolve_relative(from_dir, spec)
}

fn resolve_relative(from_dir: &Path, spec: &str) -> Option<PathBuf> {
    let candidate = from_dir.join(spec);
    try_extensions(&candidate)
}

/// Try to find a file at the given path, with various extensions.
fn try_extensions(candidate: &Path) -> Option<PathBuf> {
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

/// Get a path relative to the root.
pub fn path_relative_to(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
    .replace('\\', "/")
}

fn canonicalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_relative_to() {
        let root = Path::new("/project");
        let path = Path::new("/project/src/index.ts");
        assert_eq!(path_relative_to(root, path), "src/index.ts");
    }

    #[test]
    fn test_path_relative_to_no_prefix() {
        let root = Path::new("/other");
        let path = Path::new("/project/src/index.ts");
        assert_eq!(path_relative_to(root, path), "/project/src/index.ts");
    }

    #[test]
    fn test_parse_tsconfig_paths() {
        let mut resolver = Resolver::new(Path::new("/project"));
        // Simulate loading from a tsconfig-like JSON string.
        let tsconfig_content = r#"{
            "compilerOptions": {
                "paths": {
                    "@/*": ["./src/*"],
                    "@payload-config": ["./src/payload.config.ts"]
                }
            }
        }"#;

        // Write a temp tsconfig and load it.
        let tmp = std::env::temp_dir().join("statico-test-tsconfig.json");
        std::fs::write(&tmp, tsconfig_content).unwrap();
        resolver.load_tsconfig_paths(&tmp);
        let _ = std::fs::remove_file(&tmp);

        let aliases = resolver.aliases();
        assert!(aliases.len() >= 1, "should have at least 1 alias");

        // Check that @/ prefix was parsed.
        let at_alias = aliases.iter().find(|a| a.prefix == "@/");
        assert!(at_alias.is_some(), "should have @/ alias");
        assert_eq!(
            at_alias.unwrap().targets,
            vec!["./src/".to_string()]
        );
    }
}
