//! Import resolution and path helpers.
//!
//! Handles:
//!   - Relative imports (`./foo`, `../bar`)
//!   - tsconfig `paths` aliases (`@/components/foo` → `./src/components/foo`)
//!   - Extension resolution (try `.ts`, `.tsx`, `.js`, `.jsx`, `index.ts`, etc.)

use std::collections::HashSet;
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
/// Path aliases loaded from a single tsconfig.json.
struct TsconfigScope {
    /// Directory containing this tsconfig, relative to root (e.g. "apps/web").
    dir_rel: String,
    /// Path aliases from this tsconfig's compilerOptions.paths.
    aliases: Vec<PathAlias>,
}

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
    oxc_scopes: std::sync::Arc<OxcScopes>,
}

/// Collection of per-tsconfig oxc_resolver instances for scoped resolution.
#[cfg(feature = "deep-resolution")]
struct OxcScopes {
    /// Global resolver using root tsconfig (or default if none).
    global: oxc_resolver::Resolver,
    /// Per-directory resolvers: (tsconfig_dir_relative, resolver).
    scopes: Vec<(String, oxc_resolver::Resolver)>,
    /// Workspace aliases for all scopes.
    workspace_aliases: Vec<(String, Vec<oxc_resolver::AliasValue>)>,
}

#[cfg(not(feature = "deep-resolution"))]
impl Clone for Resolver {
    fn clone(&self) -> Self {
        Self {
            aliases: self.aliases.clone(),
            scopes: self.scopes.clone(),
            root: self.root.clone(),
        }
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
            let scopes = build_oxc_scopes(root);
            Self {
                aliases: Vec::new(),
                scopes: Vec::new(),
                root: root.to_path_buf(),
                oxc_scopes: std::sync::Arc::new(scopes),
            }
        }
        #[cfg(not(feature = "deep-resolution"))]
        Self {
            aliases: Vec::new(),
            scopes: Vec::new(),
            root: root.to_path_buf(),
        }
    }

    /// Load path aliases from a tsconfig.json file (if it exists).
    pub fn load_tsconfig_paths(&mut self, tsconfig_path: &Path) {
        let content = match std::fs::read_to_string(tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let tsconfig: serde_json::Value = match serde_json::from_str(&strip_jsonc(&content)) {
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

    /// Load tsconfig path aliases from ALL tsconfig.json files in the repo.
    /// Sub-project tsconfig files (e.g. apps/api/tsconfig.json) define their own
    /// `@/*` aliases relative to their own directory. We convert these to
    /// root-relative paths so the resolver can match them.
    pub fn load_all_tsconfig_paths(&mut self) {
        for entry in walkdir::WalkDir::new(&*self.root)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() || path.file_name().map_or(true, |n| n != "tsconfig.json") {
                continue;
            }
            let rel = path_relative_to(&self.root, path);
            // Skip node_modules.
            if rel.contains("node_modules") {
                continue;
            }
            // Skip generated/build directories and test fixtures.
            if rel.contains(".svelte-kit") || rel.contains("dist/")
                || rel.contains(".next/") || rel.contains(".nuxt/")
                || rel.contains("fixtures/") || rel.contains("test-fixtures")
                || rel.contains("__test__")
            {
                continue;
            }
            // Skip the root tsconfig — already loaded separately.
            if rel == "tsconfig.json" {
                continue;
            }
            self.load_tsconfig_paths_relative(path);
        }
    }

    /// Load tsconfig paths from a sub-project tsconfig, converting relative paths
    /// to root-relative. E.g. in `apps/api/tsconfig.json`, `./src/*` becomes
    /// `./apps/api/src/*`.
    fn load_tsconfig_paths_relative(&mut self, tsconfig_path: &Path) {
        let content = match std::fs::read_to_string(tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let tsconfig: serde_json::Value = match serde_json::from_str(&strip_jsonc(&content)) {
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

        // Directory containing this tsconfig, relative to root.
        let tsconfig_dir = match tsconfig_path.parent() {
            Some(d) => d,
            None => return,
        };
        let tsconfig_dir_rel = path_relative_to(&self.root, tsconfig_dir);

        // Also get baseUrl if present.
        let base_url = tsconfig
            .get("compilerOptions")
            .and_then(|co| co.get("baseUrl"))
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        for (pattern, targets) in paths {
            let is_wildcard = pattern.ends_with('*');
            let prefix = if is_wildcard {
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
                            let bare = if t.ends_with('*') {
                                t.trim_end_matches('*').to_string()
                            } else {
                                t.clone()
                            };
                            // Convert to root-relative: join tsconfig dir + (baseUrl + target)
                            let resolved = if bare.starts_with('.') {
                                format!("./{}/{}", tsconfig_dir_rel, bare.trim_start_matches("./"))
                            } else {
                                format!("./{}/{}/{}", tsconfig_dir_rel, base_url.trim_start_matches("./"), bare.trim_start_matches("./"))
                            };
                            // Normalize: remove double slashes, ././ etc.
                            resolved
                                .replace("././", "./")
                                .replace("//", "/")
                        })
                        .collect()
                })
                .unwrap_or_default();

            if !prefix.is_empty() && !target_list.is_empty() {
                // Add to global aliases (for backward compat / non-scoped resolution).
                // Don't deduplicate — scoped resolution will pick the right one.
                self.aliases.push(PathAlias {
                    prefix: prefix.clone(),
                    targets: target_list.clone(),
                });
            }
        }

        // Store as a scoped alias for per-file resolution.
        let scoped_aliases: Vec<PathAlias> = paths
                .iter()
                .filter_map(|(pattern, targets)| {
                    let is_wildcard = pattern.ends_with('*');
                    let prefix = if is_wildcard {
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
                                    let bare = if t.ends_with('*') {
                                        t.trim_end_matches('*').to_string()
                                    } else {
                                        t.clone()
                                    };
                                    let resolved = if bare.starts_with('.') {
                                        format!("./{}/{}", tsconfig_dir_rel, bare.trim_start_matches("./"))
                                    } else {
                                        format!("./{}/{}/{}", tsconfig_dir_rel, base_url.trim_start_matches("./"), bare.trim_start_matches("./"))
                                    };
                                    resolved.replace("././", "./").replace("//", "/")
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if !prefix.is_empty() && !target_list.is_empty() {
                        Some(PathAlias { prefix, targets: target_list })
                    } else {
                        None
                    }
                })
                .collect();

            if !scoped_aliases.is_empty() {
                self.scopes.push(TsconfigScope {
                    dir_rel: tsconfig_dir_rel.clone(),
                    aliases: scoped_aliases,
                });
            }
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

            // Package dir is the parent of package.json.
            let pkg_dir = match path.parent() {
                Some(d) => d,
                None => continue,
            };

            let main = pkg
                .get("main")
                .and_then(|v| v.as_str())
                .or_else(|| {
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
                self.aliases.push(PathAlias {
                    prefix: format!("{}/", name),
                    targets: vec![pkg_dir_rel],
                });

                // Map bare @scope/pkg → main entry file.
                self.aliases.push(PathAlias {
                    prefix: name.to_string(),
                    targets: vec![format!("./{}", target_rel)],
                });
            } else {
                // No main or index — map both bare name and name/ to package dir.
                let pkg_dir_rel = format!("./{}/", path_relative_to(&self.root, pkg_dir));
                self.aliases.push(PathAlias {
                    prefix: format!("{}/", name),
                    targets: vec![pkg_dir_rel.clone()],
                });
                self.aliases.push(PathAlias {
                    prefix: name.to_string(),
                    targets: vec![pkg_dir_rel],
                });
            }

            // Parse package.json "exports" field for sub-path aliases.
            // Handles patterns like "./lib/*" -> "./src/lib/*" where * is a wildcard.
            if let Some(exports) = pkg.get("exports").and_then(|v| v.as_object()) {
                let pkg_dir_rel = format!("./{}/", path_relative_to(&self.root, pkg_dir));
                for (export_key, export_val) in exports {
                    // Handle both simple string and conditional (dict) exports.
                    // e.g. { ".": { "import": "./src/index.js", "default": "./src/index.js" } }
                    let target_str = match export_val.as_str() {
                        Some(s) => s.to_string(),
                        None => {
                            // Try conditional exports: prefer "import", then "default", then first value.
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
                    // Strip leading "." from both key and target.
                    let sub_path = export_key.strip_prefix('.').unwrap_or(export_key);
                    let target_sub = target_str.strip_prefix('.').unwrap_or(&target_str);
                    let target_sub = target_sub.strip_prefix('/').unwrap_or(target_sub);

                    if sub_path.contains('*') {
                        // Wildcard pattern: "./lib/*" -> prefix "@scope/pkg/lib/"
                        let prefix_part = sub_path.split('*').next().unwrap_or("");
                        let target_part = target_sub.split('*').next().unwrap_or("");
                        let prefix = format!("{}{}", name, prefix_part);
                        let target = format!("{}{}", pkg_dir_rel, target_part);
                        self.aliases.push(PathAlias {
                            prefix,
                            targets: vec![target],
                        });
                    } else {
                        // Exact mapping: "./setupVitest" -> "./src/setupVitest.ts"
                        let prefix = if sub_path.is_empty() {
                            name.to_string()
                        } else {
                            format!("{}{}", name, sub_path)
                        };
                        let target = format!("{}{}", pkg_dir_rel, target_sub);
                        self.aliases.push(PathAlias {
                            prefix,
                            targets: vec![target],
                        });
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
            if let Some(resolved) = self.resolve_oxc(from_dir, spec) {
                return Some(resolved);
            }
        }

        // 1. Try relative imports first.
        if spec.starts_with('.') || spec.starts_with('/') {
            return resolve_relative(from_dir, spec);
        }

        // 2. Try scoped tsconfig path aliases (nearest tsconfig first).
        if let Some(resolved) = self.resolve_scoped(from_dir, spec) {
            return Some(resolved);
        }

        // 3. Try global tsconfig path aliases.
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

        // 4. Try as relative to root (for bare specifiers like "src/foo").
        let from_root = self.root.join(spec);
        if let Some(found) = try_extensions(&from_root) {
            return Some(found);
        }

        None
    }
    /// Try resolving using the nearest tsconfig scope's aliases.
    fn resolve_scoped(&self, from_dir: &Path, spec: &str) -> Option<PathBuf> {
        let from_rel = path_relative_to(&self.root, from_dir);

        // Find the nearest scope (longest matching prefix).
        let best_scope = self.scopes
            .iter()
            .filter(|scope| from_rel.starts_with(&scope.dir_rel))
            .max_by_key(|scope| scope.dir_rel.len());

        let scope = best_scope?;

        for alias in &scope.aliases {
            if let Some(rest) = spec.strip_prefix(&alias.prefix) {
                for target in &alias.targets {
                    let resolved_path = self.root.join(target).join(rest);
                    if let Some(found) = try_extensions(&resolved_path) {
                        return Some(found);
                    }
                }
            }
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

/// Strip JSONC comments (// and /* */) from a string so serde_json can parse it.
/// Handles strings correctly — comments inside string literals are preserved.
fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;

    while i < chars.len() {
        if in_string {
            out.push(chars[i]);
            if chars[i] == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
        } else if chars[i] == '"' {
            in_string = true;
            out.push(chars[i]);
            i += 1;
        } else if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // Line comment — skip to end of line.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // Block comment — skip to */.
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // skip */
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
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
// oxc_resolver integration
// ---------------------------------------------------------------------------

/// Build oxc_resolver scopes: one global + one per tsconfig sub-directory.
/// Each scope uses its own tsconfig for correct per-package path resolution,
/// so `@/` in `apps/api/v2/` resolves differently than `@/` in `packages/atoms/`.
#[cfg(feature = "deep-resolution")]
fn build_oxc_scopes(root: &Path) -> OxcScopes {
    use oxc_resolver::{AliasValue, ResolveOptions, TsconfigDiscovery, TsconfigOptions, TsconfigReferences};

    let workspace_aliases = build_workspace_aliases(root);
    let root_str = root.to_string_lossy().to_string();

    let default_opts = ResolveOptions {
        extensions: vec![
            ".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into(),
            ".mjs".into(), ".cjs".into(),
        ],
        main_fields: vec!["types".into(), "typings".into(), "module".into(), "main".into()],
        condition_names: vec![
            "import".into(), "module".into(), "require".into(),
            "default".into(), "types".into(), "node".into(),
        ],
        alias: workspace_aliases.clone(),
        modules: vec!["node_modules".into(), root_str.clone()],
        ..ResolveOptions::default()
    };

    // Global resolver (no tsconfig or root tsconfig).
    let global_tsconfig = root.join("tsconfig.json");
    let global_opts = if global_tsconfig.exists() {
        ResolveOptions {
            tsconfig: Some(TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: global_tsconfig.clone(),
                references: TsconfigReferences::Auto,
            })),
            ..default_opts.clone()
        }
    } else {
        default_opts.clone()
    };
    let global = oxc_resolver::Resolver::new(global_opts);

    // Per-scope resolvers from sub-project tsconfig files.
    let mut scopes: Vec<(String, oxc_resolver::Resolver)> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.file_name().map_or(true, |n| n != "tsconfig.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
        if rel.contains("node_modules") || rel.contains(".svelte-kit")
            || rel.contains("dist/") || rel.contains(".next/")
            || rel == "tsconfig.json"
        {
            continue;
        }
        let tsconfig_dir = match path.parent() {
            Some(d) => d,
            None => continue,
        };
        let tsconfig_dir_rel = path_relative_to(root, tsconfig_dir);

        let opts = ResolveOptions {
            tsconfig: Some(TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: path.to_path_buf(),
                references: TsconfigReferences::Auto,
            })),
            ..default_opts.clone()
        };
        scopes.push((tsconfig_dir_rel, oxc_resolver::Resolver::new(opts)));
    }

    OxcScopes {
        global,
        scopes,
        workspace_aliases,
    }
}

/// Build oxc-compatible alias list from workspace package.json files.
#[cfg(feature = "deep-resolution")]
fn build_workspace_aliases(root: &Path) -> Vec<(String, Vec<oxc_resolver::AliasValue>)> {
    use oxc_resolver::AliasValue;

    let mut aliases: Vec<(String, Vec<AliasValue>)> = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.file_name().map_or(true, |n| n != "package.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
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
        if !name.starts_with('@') && !name.contains('/') {
            continue;
        }

        let pkg_dir = path.parent().unwrap();
        aliases.push((
            format!("{}$", name),
            vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())],
        ));
        aliases.push((
            name.to_string(),
            vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())],
        ));
    }

    aliases
}

impl Resolver {
    /// Try oxc resolution using the nearest tsconfig scope.
    #[cfg(feature = "deep-resolution")]
    fn resolve_oxc(&self, from_dir: &Path, spec: &str) -> Option<PathBuf> {
        let from_rel = path_relative_to(&self.root, from_dir);

        // Find the nearest scope (longest matching prefix).
        let best_scope = self.oxc_scopes.scopes
            .iter()
            .filter(|(dir_rel, _)| from_rel.starts_with(dir_rel))
            .max_by_key(|(dir_rel, _)| dir_rel.len())
            .map(|(_, resolver)| resolver);

        let resolver = best_scope.unwrap_or(&self.oxc_scopes.global);

        match resolver.resolve(from_dir, spec) {
            Ok(resolution) => {
                let path = resolution.full_path();
                if let Ok(rel) = path.strip_prefix(&self.root) {
                    Some(self.root.join(rel))
                } else {
                    Some(path.to_path_buf())
                }
            }
            Err(_) => None,
        }
    }
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

/// Find directories containing published packages (not private).
/// Returns a set of relative directory paths like "packages/common", "packages/core".
/// Only includes packages under "packages/" or at root level — excludes examples,
/// demos, test directories, and apps that happen to have package.json files.
pub fn find_published_package_dirs(root: &Path) -> HashSet<String> {
    let mut published = HashSet::new();
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.file_name().map_or(true, |n| n != "package.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
        if rel.contains("node_modules") {
            continue;
        }
        // Only include packages in "packages/" directories.
        // Skip examples/, demos/, test/, apps/ etc.
        let pkg_rel = rel.replace("\\", "/");
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
