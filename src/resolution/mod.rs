//! Import resolution and path helpers.
//!
//! Handles:
//!   - Relative imports (`./foo`, `../bar`)
//!   - tsconfig `paths` aliases (`@/components/foo` → `./src/components/foo`)
//!   - Extension resolution (try `.ts`, `.tsx`, `.js`, `.jsx`, `index.ts`, etc.)

mod paths;
mod tsconfig;

#[cfg(feature = "deep-resolution")]
mod oxc;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use paths::resolve_relative;

/// Check that a resolved path is within the project root.
/// Uses the same logic as `ensure_within_root` but returns bool.
fn is_within_root(root: &Path, resolved: &Path) -> bool {
    if let (Ok(canonical), Ok(canonical_root)) =
        (std::fs::canonicalize(resolved), std::fs::canonicalize(root))
    {
        return canonical.starts_with(&canonical_root);
    }
    // Fallback: lexical check
    match resolved.strip_prefix(root) {
        Ok(suffix) => {
            let mut depth = 0i32;
            for component in suffix.components() {
                match component {
                    std::path::Component::ParentDir => {
                        depth -= 1;
                        if depth < 0 { return false; }
                    }
                    std::path::Component::Normal(_) => { depth += 1; }
                    _ => {}
                }
            }
            depth >= 0
        }
        Err(_) => false,
    }
}
use tsconfig::{parse_tsconfig_paths, parse_tsconfig_paths_relative, resolve_scoped, TsconfigScope};

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
pub struct Resolver {
    /// Global aliases (from root tsconfig + workspace packages).
    aliases: Vec<PathAlias>,
    /// Per-tsconfig scoped aliases for per-package path resolution.
    /// When resolving from a file, the nearest tsconfig scope is used first.
    scopes: Vec<TsconfigScope>,
    root: PathBuf,
    /// Per-scope oxc_resolver instances, keyed by tsconfig directory (relative to root).
    /// Each scope uses its nearest tsconfig.json for proper path resolution,
    /// so `@/` in `apps/api/v2/` resolves differently than `@/` in `packages/platform/atoms/`.
    #[cfg(feature = "deep-resolution")]
    oxc_scopes: std::sync::Arc<oxc::OxcScopes>,
}

#[cfg(not(feature = "deep-resolution"))]
impl Clone for Resolver {
    fn clone(&self) -> Self {
        Self { aliases: self.aliases.clone(), scopes: self.scopes.clone(), root: self.root.clone() }
    }
}

#[cfg(feature = "deep-resolution")]
impl Clone for Resolver {
    fn clone(&self) -> Self {
        Self {
            aliases: self.aliases.clone(),
            scopes: self.scopes.clone(),
            root: self.root.clone(),
            oxc_scopes: self.oxc_scopes.clone(), // Arc cloning is cheap
        }
    }
}

impl Resolver {
    /// Create a new resolver for the given project root.
    pub fn new(root: &Path) -> Self {
        #[cfg(feature = "deep-resolution")]
        {
            let scopes = oxc::build_oxc_scopes(root);
            Self {
                aliases: Vec::new(),
                scopes: Vec::new(),
                root: root.to_path_buf(),
                oxc_scopes: std::sync::Arc::new(scopes),
            }
        }
        #[cfg(not(feature = "deep-resolution"))]
        Self { aliases: Vec::new(), scopes: Vec::new(), root: root.to_path_buf() }
    }

    /// Load path aliases from a tsconfig.json file (if it exists).
    pub fn load_tsconfig_paths(&mut self, tsconfig_path: &Path) {
        if let Some(aliases) = parse_tsconfig_paths(tsconfig_path) {
            self.aliases.extend(aliases);
        }
    }

    /// Get the loaded aliases (for testing/debugging).
    pub fn aliases(&self) -> &[PathAlias] {
        &self.aliases
    }

    /// Load tsconfig path aliases from ALL tsconfig.json files in the repo.
    /// Sub-project tsconfig files (e.g. apps/api/tsconfig.json) define their own
    /// `@/*` aliases relative to their own directory. We convert these to
    /// root-relative paths so the resolver can match them.
    pub fn load_all_tsconfig_paths(&mut self) {
        for entry in walkdir::WalkDir::new(&*self.root).max_depth(6).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.file_name().is_none_or(|n| n != "tsconfig.json") {
                continue;
            }
            let rel = path_relative_to(&self.root, path);
            // Skip node_modules.
            if rel.contains("node_modules") {
                continue;
            }
            // Skip generated/build directories and test fixtures.
            if rel.contains(".svelte-kit")
                || rel.contains("dist/")
                || rel.contains(".next/")
                || rel.contains(".nuxt/")
                || rel.contains("fixtures/")
                || rel.contains("test-fixtures")
                || rel.contains("__test__")
            {
                continue;
            }
            // Skip the root tsconfig — already loaded separately.
            if rel == "tsconfig.json" {
                continue;
            }
            if let Some((global_aliases, scope)) = parse_tsconfig_paths_relative(path, &self.root) {
                self.aliases.extend(global_aliases);
                self.scopes.push(scope);
            }
        }
    }

    /// Load workspace package mappings from all package.json files under the root.
    /// Maps `@scope/name` → `<root>/<pkg_dir>/src/index.ts` (or whatever `main` points to).
    pub fn load_workspace_packages(&mut self) {
        // Walk all package.json files under the root.
        for entry in walkdir::WalkDir::new(&*self.root).max_depth(5).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.file_name().is_none_or(|n| n != "package.json") {
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

            // Package dir is the parent of package.json.
            let pkg_dir = match path.parent() {
                Some(d) => d,
                None => continue,
            };

            let main = pkg.get("main").and_then(|v| v.as_str()).or_else(|| {
                // No main field — try index.ts/index.tsx in package dir.
                if pkg_dir.join("index.ts").exists() {
                    Some("index.ts")
                } else if pkg_dir.join("index.tsx").exists() {
                    Some("index.tsx")
                } else {
                    None
                }
            });

            // Check if the main entry file actually exists.
            let main_exists = main.as_ref().is_some_and(|m| pkg_dir.join(m).is_file());

            if main_exists {
                let main_val = main.unwrap();
                let target = pkg_dir.join(main_val);
                let target_rel = path_relative_to(&self.root, &target);

                // Map @scope/pkg/ → package dir (for sub-path imports).
                let pkg_dir_rel = format!("./{}/", path_relative_to(&self.root, pkg_dir));
                self.aliases.push(PathAlias { prefix: format!("{}/", name), targets: vec![pkg_dir_rel] });

                // Map bare @scope/pkg → main entry file.
                self.aliases.push(PathAlias { prefix: name.to_string(), targets: vec![format!("./{}", target_rel)] });
            } else {
                // No main or index — map both bare name and name/ to package dir.
                let pkg_dir_rel = format!("./{}/", path_relative_to(&self.root, pkg_dir));
                self.aliases.push(PathAlias { prefix: format!("{}/", name), targets: vec![pkg_dir_rel.clone()] });
                self.aliases.push(PathAlias { prefix: name.to_string(), targets: vec![pkg_dir_rel] });
            }

            // Parse package.json "exports" field for sub-path aliases.
            if let Some(exports) = pkg.get("exports").and_then(|v| v.as_object()) {
                let pkg_dir_rel = format!("./{}/", path_relative_to(&self.root, pkg_dir));
                for (export_key, export_val) in exports {
                    let target_str = match export_val.as_str() {
                        Some(s) => s.to_string(),
                        None => {
                            let obj = match export_val.as_object() {
                                Some(o) => o,
                                None => continue,
                            };
                            if let Some(s) = obj.get("import").and_then(|v| v.as_str()) {
                                s.to_string()
                            } else if let Some(s) = obj.get("default").and_then(|v| v.as_str()) {
                                s.to_string()
                            } else if let Some(s) = obj.values().find_map(|v| v.as_str()) {
                                s.to_string()
                            } else {
                                continue;
                            }
                        }
                    };
                    let sub_path = export_key.strip_prefix('.').unwrap_or(export_key);
                    let target_sub = target_str.strip_prefix('.').unwrap_or(&target_str);
                    let target_sub = target_sub.strip_prefix('/').unwrap_or(target_sub);

                    if sub_path.contains('*') {
                        let prefix_part = sub_path.split('*').next().unwrap_or("");
                        let target_part = target_sub.split('*').next().unwrap_or("");
                        let prefix = format!("{}{}", name, prefix_part);
                        let target = format!("{}{}", pkg_dir_rel, target_part);
                        self.aliases.push(PathAlias { prefix, targets: vec![target] });
                    } else {
                        let prefix =
                            if sub_path.is_empty() { name.to_string() } else { format!("{}{}", name, sub_path) };
                        let target = format!("{}{}", pkg_dir_rel, target_sub);
                        self.aliases.push(PathAlias { prefix, targets: vec![target] });
                    }
                }
            }
        }
    }

    /// Resolve an import specifier relative to the given file directory.
    /// Returns the resolved absolute path, or None if unresolvable.
    pub fn resolve(&self, from_dir: &Path, spec: &str) -> Option<PathBuf> {
        // 0. Try oxc_resolver with per-scope resolution.
        #[cfg(feature = "deep-resolution")]
        {
            if let Some(resolved) = oxc::resolve_oxc(&self.oxc_scopes, &self.root, from_dir, spec) {
                return Some(resolved);
            }
        }

        // 1. Try relative imports first.
        if spec.starts_with('.') || spec.starts_with('/') {
            if let Some(resolved) = resolve_relative(from_dir, spec) {
                // V-3: Verify resolved path stays within project root
                if is_within_root(&self.root, &resolved) {
                    return Some(resolved);
                }
            }
            return None;
        }

        // 2. Try scoped tsconfig path aliases (nearest tsconfig first).
        if let Some(resolved) = resolve_scoped(&self.scopes, &self.root, from_dir, spec) {
            return Some(resolved);
        }

        // 3. Try global tsconfig path aliases.
        for alias in &self.aliases {
            if let Some(rest) = spec.strip_prefix(&alias.prefix) {
                for target in &alias.targets {
                    let resolved_path = self.root.join(target).join(rest);
                    if let Some(found) = paths::try_extensions(&resolved_path) {
                        return Some(found);
                    }
                }
            }
        }

        // 4. Try as relative to root (for bare specifiers like "src/foo").
        let from_root = self.root.join(spec);
        if let Some(found) = paths::try_extensions(&from_root) {
            return Some(found);
        }

        None
    }
}

/// Resolve a relative import specifier to an actual file path.
pub fn resolve_import(from_dir: &Path, spec: &str) -> Option<PathBuf> {
    resolve_relative(from_dir, spec)
}

/// Get a path relative to the root.
pub fn path_relative_to(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
    .replace('\\', "/")
}

/// Find directories containing published packages (not private).
/// Returns a set of relative directory paths like "packages/common", "packages/core".
/// Only includes packages under "packages/" or at root level — excludes examples,
/// demos, test directories, and apps that happen to have package.json files.
pub fn find_published_package_dirs(root: &Path) -> HashSet<String> {
    let mut published = HashSet::new();
    for entry in walkdir::WalkDir::new(root).max_depth(4).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.file_name().is_none_or(|n| n != "package.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
        if rel.contains("node_modules") {
            continue;
        }
        // Only include packages in "packages/" directories.
        let pkg_rel = rel.replace('\\', "/");
        if !pkg_rel.starts_with("packages/") && !pkg_rel.starts_with("./packages/") {
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
        // Skip private packages and packages without a name
        let private = pkg.get("private").and_then(|v| v.as_bool()).unwrap_or(false);
        let has_name = pkg.get("name").and_then(|v| v.as_str()).is_some();
        if private || !has_name {
            continue;
        }
        // The package directory is the parent of package.json
        if let Some(pkg_dir) = path.parent() {
            let dir_rel = path_relative_to(root, pkg_dir);
            published.insert(dir_rel);
        }
    }
    published
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
        assert!(!aliases.is_empty(), "should have at least 1 alias");

        // Check that @/ prefix was parsed.
        let at_alias = aliases.iter().find(|a| a.prefix == "@/");
        assert!(at_alias.is_some(), "should have @/ alias");
        assert_eq!(at_alias.unwrap().targets, vec!["./src/".to_string()]);
    }
}
