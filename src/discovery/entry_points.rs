//! Entry point detection for framework, implicit, and public API files.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use crate::frameworks;

/// Categorized entry points: framework entries vs. implicit/tooling entries.
pub struct EntryPoints {
    /// Files whose exports are consumed by the framework (pages, routes, layouts, etc.).
    pub framework: BTreeSet<String>,
    /// Tooling/config/scripts (tests, scripts, generated files, migrations, etc.).
    /// Their exports should still be checked for usage.
    pub implicit: BTreeSet<String>,
    /// Package public API files — files referenced by package.json `main`/`module`/`exports`
    /// in ANY workspace package. Their exports form the package's public API and should
    /// NOT be flagged as unused, even if no internal file imports them.
    pub public_api: BTreeSet<String>,
}

impl EntryPoints {
    /// Union of framework, implicit, and public API entries.
    pub fn all(&self) -> BTreeSet<String> {
        self.framework
            .union(&self.implicit)
            .cloned()
            .collect::<BTreeSet<_>>()
            .union(&self.public_api)
            .cloned()
            .collect()
    }
}

/// Detect entry points using framework profiles + package.json/tsconfig + defaults.
/// Returns categorized entry points separating framework entries from implicit ones.
pub fn discover_entry_points(root: &Path, source_files: &[(String, String)]) -> EntryPoints {
    let mut framework: BTreeSet<String> = BTreeSet::new();
    let mut implicit: BTreeSet<String> = BTreeSet::new();
    let mut public_api: BTreeSet<String> = BTreeSet::new();
    let source_set: HashSet<&str> = source_files.iter().map(|(p, _)| p.as_str()).collect();

    // 1. Detect active framework profiles.
    let profiles = frameworks::detect_profiles(root);

    // 2. Apply profile entry matchers against all source files → framework entries.
    for (rel, _) in source_files {
        for profile in &profiles {
            if profile.entry_matchers.iter().any(|m| m.matches(rel)) {
                framework.insert(rel.clone());
            }
        }
    }

    // 3. Apply profile implicit matchers against all source files → implicit entries.
    for (rel, _) in source_files {
        for profile in &profiles {
            if profile.implicit_matchers.iter().any(|m| m.matches(rel)) {
                implicit.insert(rel.clone());
            }
        }
    }

    // 4. package.json main/module/browser/exports fields → framework entries.
    add_package_json_entries(root, &source_set, &mut framework);

    // 4b. For monorepos, also discover entry points from each workspace package.
    add_workspace_entries(root, &source_set, &mut framework, &mut public_api);

    // 5. tsconfig.json files field → framework entries.
    add_tsconfig_entries(root, &mut framework);

    // 6. Default entry point locations → framework entries.
    add_default_entries(&source_set, &mut framework);

    // 6b. Rust workspace crates: find Cargo.toml files and add their src/lib.rs, src/main.rs.
    super::rust::add_rust_crate_entries(root, &source_set, &mut framework);

    // 6c. Parse Cargo.toml for [lib] path and [[bin]] targets → entry points.
    super::rust::add_rust_cargo_entries(root, &source_set, &mut framework);

    // 7. Tooling & config scripts → implicit entries (loaded by config, not ES imports).
    super::tooling::add_tooling_entries(source_files, &mut implicit);

    // 7b. Rust implicit entries: build.rs, tests/, benches/, examples/, fuzz/.
    // Also src/bin/ files as FRAMEWORK entries (they're runtime binaries).
    super::rust::add_rust_implicit_entries(source_files, &mut implicit, &mut framework);

    EntryPoints { framework, implicit, public_api }
}

// ---------------------------------------------------------------------------
// package.json / tsconfig / default entries (not framework-specific)
// ---------------------------------------------------------------------------

fn add_package_json_entries(root: &Path, source_set: &HashSet<&str>, entry_points: &mut BTreeSet<String>) {
    let content = match std::fs::read_to_string(root.join("package.json")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    for field in &["main", "module", "browser"] {
        if let Some(val) = pkg.get(field).and_then(|v| v.as_str()) {
            let rel = normalize_entry(val);
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            } else {
                let ts_rel = rel.replace(".js", ".ts").replace(".jsx", ".tsx");
                if source_set.contains(ts_rel.as_str()) {
                    entry_points.insert(ts_rel);
                } else {
                    entry_points.insert(rel);
                }
            }
        }
    }

    if let Some(exports) = pkg.get("exports") {
        extract_exports_paths(exports, entry_points);
    }
}

fn add_tsconfig_entries(root: &Path, entry_points: &mut BTreeSet<String>) {
    let content = match std::fs::read_to_string(root.join("tsconfig.json")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let tsconfig: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(files) = tsconfig.get("files").and_then(|v| v.as_array()) {
        for f in files {
            if let Some(s) = f.as_str() {
                entry_points.insert(s.to_string());
            }
        }
    }
}

/// For monorepos, discover entry points from each workspace package.
/// Looks for package.json main/module/exports fields and default entry files
/// within each workspace package directory.
fn add_workspace_entries(
    root: &Path,
    source_set: &HashSet<&str>,
    entry_points: &mut BTreeSet<String>,
    public_api: &mut BTreeSet<String>,
) {
    let mono = match crate::workspace::detect_monorepo(root) {
        Some(m) => m,
        None => return, // Not a monorepo, nothing to do.
    };

    let defaults = [
        "src/index.ts",
        "src/index.tsx",
        "src/main.ts",
        "src/main.tsx",
        "index.ts",
        "main.ts",
        "src/main.rs",
        "src/lib.rs",
    ];

    // Enumerate actual package directories from the workspace prefixes.
    let mut pkg_dirs: Vec<String> = Vec::new();
    for prefix in &mono.packages {
        let prefix_path = root.join(prefix);
        if !prefix_path.is_dir() {
            continue;
        }
        // If prefix is a leaf directory (not a glob pattern like packages/*),
        // check if it has a package.json — it's a package itself.
        if prefix_path.join("package.json").exists() {
            pkg_dirs.push(prefix.clone());
            continue;
        }
        // Enumerate subdirectories as potential packages.
        for entry in std::fs::read_dir(&prefix_path).ok().into_iter().flatten() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Skip hidden and known non-package dirs.
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            pkg_dirs.push(format!("{}/{}", prefix.trim_end_matches('/'), name));
        }
    }

    for pkg_dir in &pkg_dirs {
        // Try package.json main/module/exports fields.
        let pkg_json_path = root.join(pkg_dir).join("package.json");
        if let Ok(content) = std::fs::read_to_string(&pkg_json_path)
            && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
        {
            // main/module → both entry_point and public_api
            for field in &["main", "module"] {
                if let Some(val) = pkg.get(field).and_then(|v| v.as_str()) {
                    let rel = format!("{}/{}", pkg_dir, val.trim_start_matches("./"));
                    let resolved = resolve_to_source(&rel, source_set);
                    if let Some(r) = resolved {
                        entry_points.insert(r.clone());
                        public_api.insert(r);
                    }
                }
            }
            // exports field → public_api (these define the package's external interface)
            if let Some(exports) = pkg.get("exports") {
                let mut export_paths = BTreeSet::new();
                extract_exports_paths(exports, &mut export_paths);
                for ep in &export_paths {
                    let rel = format!("{}/{}", pkg_dir, ep.trim_start_matches("./"));
                    let resolved = resolve_to_source(&rel, source_set);
                    if let Some(r) = resolved {
                        public_api.insert(r);
                    }
                }
            }
        }

        // Try default entry points within the package dir.
        for def in &defaults {
            let rel = format!("{}/{}", pkg_dir, def);
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }

        // Nx-specific: try project.json for build targets and sourceRoot.
        let project_json_path = root.join(pkg_dir).join("project.json");
        if project_json_path.exists()
            && let Some(project) = crate::frameworks::monorepo_nx::parse_project_json(&project_json_path)
            && let Some(entry_rel) =
                crate::frameworks::monorepo_nx::nx_project_entry_path(root, &root.join(pkg_dir), &project)
        {
            let resolved = resolve_to_source(&entry_rel, source_set);
            if let Some(r) = resolved {
                entry_points.insert(r.clone());
                if project.project_type.as_deref() == Some("library") {
                    public_api.insert(r);
                }
            } else {
                entry_points.insert(entry_rel);
            }
        }
    }
}

fn add_default_entries(source_set: &HashSet<&str>, entry_points: &mut BTreeSet<String>) {
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
        "src/main.rs",
        "src/lib.rs",
    ];
    for def in &defaults {
        if source_set.contains(*def) {
            entry_points.insert(def.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_exports_paths(exports: &serde_json::Value, paths: &mut BTreeSet<String>) {
    match exports {
        serde_json::Value::String(s) => {
            paths.insert(s.clone());
        }
        serde_json::Value::Object(map) => {
            for val in map.values() {
                extract_exports_paths(val, paths);
            }
        }
        _ => {}
    }
}

pub(super) fn normalize_entry(path: &str) -> String {
    path.trim_start_matches("./").to_string()
}

/// Resolve a path (possibly .js/.jsx) to an actual source file in the set.
fn resolve_to_source(rel: &str, source_set: &HashSet<&str>) -> Option<String> {
    if source_set.contains(rel) {
        return Some(rel.to_string());
    }
    // Try .ts/.tsx extensions
    let ts_rel = rel.replace(".js", ".ts").replace(".jsx", ".tsx");
    if source_set.contains(ts_rel.as_str()) {
        return Some(ts_rel);
    }
    // Try appending /index.ts
    let idx = format!("{}/index.ts", rel.trim_end_matches(".ts").trim_end_matches(".js"));
    if source_set.contains(idx.as_str()) {
        return Some(idx);
    }
    None
}
